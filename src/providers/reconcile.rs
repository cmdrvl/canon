#![forbid(unsafe_code)]

//! Deterministic cross-provider reconciliation as typed evidence.
//!
//! Reconciliation packages declare exact field maps and evidence policy for
//! joining external provider namespaces without collapsing them into truth.
//! Canon core emits proposed identity, relationship, or cannot-link evidence,
//! preserves native namespaces and scopes, abstains on unsafe mismatches, and
//! keeps ambiguous states reviewable instead of writing registries implicitly.

use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

pub const CANON_PROVIDER_RECONCILE_VERSION: &str = "canon.provider.reconcile.v1";

pub type ProviderReconcileResult<T> = Result<T, ProviderReconcileError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReconcileErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingFieldMap,
    DateParse,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileError {
    pub code: ProviderReconcileErrorCode,
    pub message: String,
}

impl ProviderReconcileError {
    pub fn new(code: ProviderReconcileErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProviderReconcileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProviderReconcileError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReconcilePackageCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileEvidenceKind {
    ProposedIdentity,
    ProposedRelationship,
    CannotLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileReviewStateKind {
    MissingLink,
    OneToMany,
    StaleLink,
    ConflictingLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileAbstentionKind {
    Namespace,
    Scope,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileFieldComparator {
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsafeNamespacePolicy {
    Abstain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsafeScopePolicy {
    Abstain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsafeTypePolicy {
    Abstain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingLinkPolicy {
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OneToManyPolicy {
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleLinkPolicy {
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryWritePolicy {
    NeverImplicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcilePackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_maps: Vec<ProviderReconcileFieldMap>,
    pub evidence_policy: ProviderReconcileEvidencePolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<ProviderReconcileDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderReconcileDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileFieldMap {
    pub map_id: String,
    pub evidence_kind: ReconcileEvidenceKind,
    pub left_provider_id: String,
    pub left_namespace: String,
    pub left_scope_id: String,
    pub left_type_ref: String,
    pub right_provider_id: String,
    pub right_namespace: String,
    pub right_scope_id: String,
    pub right_type_ref: String,
    pub left_field_path: String,
    pub right_field_path: String,
    pub comparator: ReconcileFieldComparator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_term_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileEvidencePolicy {
    pub stale_after_days: u32,
    pub unsafe_namespace_policy: UnsafeNamespacePolicy,
    pub unsafe_scope_policy: UnsafeScopePolicy,
    pub unsafe_type_policy: UnsafeTypePolicy,
    pub missing_link_policy: MissingLinkPolicy,
    pub one_to_many_policy: OneToManyPolicy,
    pub stale_link_policy: StaleLinkPolicy,
    pub conflict_policy: ConflictPolicy,
    pub registry_write_policy: RegistryWritePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderReconcileMapRef {
    pub package_digest: String,
    pub map_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderNativeRecord {
    pub provider_id: String,
    pub native_namespace: String,
    pub native_id: String,
    pub scope_id: String,
    pub object_type_ref: String,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fields: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderNativeRef {
    pub provider_id: String,
    pub native_namespace: String,
    pub native_id: String,
    pub scope_id: String,
    pub object_type_ref: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileRunInput {
    pub as_of: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_records: Vec<ProviderNativeRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right_records: Vec<ProviderNativeRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileEvidenceRecord {
    pub evidence_id: String,
    pub map_id: String,
    pub evidence_kind: ReconcileEvidenceKind,
    pub match_value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_term_id: Option<String>,
    pub package_digest: String,
    pub left: ProviderNativeRef,
    pub right: ProviderNativeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileReviewState {
    pub review_id: String,
    pub map_id: String,
    pub state_kind: ReconcileReviewStateKind,
    pub match_value: String,
    pub left: ProviderNativeRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right_candidates: Vec<ProviderNativeRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_evidence_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileAbstention {
    pub abstention_id: String,
    pub map_id: String,
    pub kind: ReconcileAbstentionKind,
    pub match_value: String,
    pub left: ProviderNativeRef,
    pub right: ProviderNativeRef,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileReport {
    pub package_digest: String,
    pub as_of: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<ProviderReconcileEvidenceRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_states: Vec<ProviderReconcileReviewState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abstentions: Vec<ProviderReconcileAbstention>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registry_write_intents: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReconcileImpactSummary {
    pub proposed_identity: usize,
    pub proposed_relationship: usize,
    pub cannot_link: usize,
    pub review_states: usize,
    pub abstentions: usize,
}

pub fn provider_reconcile_schema_version() -> &'static str {
    CANON_PROVIDER_RECONCILE_VERSION
}

pub fn finalize_package(
    mut package: ProviderReconcilePackage,
) -> ProviderReconcileResult<ProviderReconcilePackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_PROVIDER_RECONCILE_VERSION.to_string();
    }
    if package.version != CANON_PROVIDER_RECONCILE_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported provider reconcile contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_identifier(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    if package.evidence_policy.stale_after_days == 0 {
        return Err(artifact_contract_error(
            "evidence_policy.stale_after_days must be greater than zero",
        ));
    }

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<ProviderReconcileResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();

    let mut field_maps = package
        .field_maps
        .into_iter()
        .map(normalize_field_map)
        .collect::<ProviderReconcileResult<Vec<_>>>()?;
    if field_maps.is_empty() {
        return Err(artifact_contract_error(
            "provider reconcile package must declare at least one field map",
        ));
    }
    field_maps.sort_by(|left, right| left.map_id.cmp(&right.map_id));

    let mut deduped: Vec<ProviderReconcileFieldMap> = Vec::with_capacity(field_maps.len());
    for map in field_maps {
        if let Some(previous) = deduped.last()
            && previous.map_id == map.map_id
        {
            if previous != &map {
                return Err(artifact_contract_error(format!(
                    "field map {} cannot be declared with conflicting content",
                    map.map_id
                )));
            }
            continue;
        }
        deduped.push(map);
    }

    package.documentation = documentation;
    package.field_maps = deduped;
    Ok(package)
}

pub fn canonical_package_bytes(
    package: &ProviderReconcilePackage,
) -> ProviderReconcileResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize provider reconcile package: {error}"
        ))
    })
}

pub fn provider_reconcile_package_digest(
    package: &ProviderReconcilePackage,
) -> ProviderReconcileResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn resolve_map_ref(
    package: &ProviderReconcilePackage,
    reference: &ProviderReconcileMapRef,
) -> ProviderReconcileResult<ProviderReconcileFieldMap> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_map_ref(reference.clone())?;
    let digest = provider_reconcile_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "reconcile map {} is pinned to {} but package resolved to {}",
            reference.map_id, reference.package_digest, digest
        )));
    }

    package
        .field_maps
        .iter()
        .find(|map| map.map_id == reference.map_id)
        .cloned()
        .ok_or_else(|| {
            missing_field_map_error(format!("unknown reconcile map {}", reference.map_id))
        })
}

pub fn validate_package_for_execution(
    package: &ProviderReconcilePackage,
    required_maps: &[ProviderReconcileMapRef],
) -> ProviderReconcileResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = provider_reconcile_package_digest(&package)?;
    for reference in required_maps {
        let reference = finalize_map_ref(reference.clone())?;
        if reference.package_digest != digest {
            return Err(compatibility_policy_error(format!(
                "reconcile map {} is pinned to {} but package resolved to {}",
                reference.map_id, reference.package_digest, digest
            )));
        }
        let _ = resolve_map_ref(&package, &reference)?;
    }
    Ok(digest)
}

pub fn package_compatibility(
    locked: &ProviderReconcilePackage,
    candidate: &ProviderReconcilePackage,
    used_maps: &[ProviderReconcileMapRef],
) -> ProviderReconcileResult<ProviderReconcilePackageCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "provider reconcile package ids differ: {} vs {}",
            locked.package_id, candidate.package_id
        )));
    }

    let locked_major = semver_major(&locked.package_version)?;
    let candidate_major = semver_major(&candidate.package_version)?;
    if locked_major != candidate_major {
        return Err(compatibility_policy_error(format!(
            "provider reconcile package {} changed major version from {} to {}",
            locked.package_id, locked_major, candidate_major
        )));
    }

    let locked_digest = provider_reconcile_package_digest(&locked)?;
    let candidate_digest = provider_reconcile_package_digest(&candidate)?;
    if locked_digest == candidate_digest {
        return Ok(ProviderReconcilePackageCompatibility::ExactDigest);
    }

    for reference in used_maps {
        let migrated = ProviderReconcileMapRef {
            package_digest: candidate_digest.clone(),
            map_id: reference.map_id.clone(),
        };
        let _ = resolve_map_ref(&candidate, &migrated)?;
    }

    Ok(ProviderReconcilePackageCompatibility::CompatibleSameMajor)
}

pub fn reconcile_records(
    package: &ProviderReconcilePackage,
    required_maps: &[ProviderReconcileMapRef],
    input: &ProviderReconcileRunInput,
) -> ProviderReconcileResult<ProviderReconcileReport> {
    let package = finalize_package(package.clone())?;
    let package_digest = validate_package_for_execution(&package, required_maps)?;
    let as_of = parse_date(&input.as_of, "as_of")?;

    let mut evidence = Vec::new();
    let mut review_states = Vec::new();
    let mut abstentions = Vec::new();

    for reference in required_maps {
        let map = resolve_map_ref(&package, reference)?;
        let left_records = input
            .left_records
            .iter()
            .filter(|record| record.provider_id == map.left_provider_id)
            .collect::<Vec<_>>();
        let right_records = input
            .right_records
            .iter()
            .filter(|record| record.provider_id == map.right_provider_id)
            .collect::<Vec<_>>();

        for left_record in left_records {
            let Some(match_value) = field_value(left_record, &map.left_field_path) else {
                continue;
            };

            let mut safe_matches = Vec::new();
            let mut stale_matches = Vec::new();
            let mut had_any_candidate = false;

            for right_record in &right_records {
                let Some(right_value) = field_value(right_record, &map.right_field_path) else {
                    continue;
                };
                if !values_match(match_value, right_value, map.comparator) {
                    continue;
                }

                had_any_candidate = true;
                if let Some((kind, message)) = mismatch_reason(left_record, right_record, &map) {
                    abstentions.push(build_abstention(
                        &map,
                        kind,
                        match_value,
                        left_record,
                        right_record,
                        message,
                    )?);
                    continue;
                }

                if is_stale(
                    left_record,
                    right_record,
                    as_of,
                    package.evidence_policy.stale_after_days,
                )? {
                    stale_matches.push(native_ref(right_record));
                    continue;
                }

                safe_matches.push(native_ref(right_record));
            }

            if safe_matches.len() == 1 {
                evidence.push(build_evidence(
                    &map,
                    match_value,
                    &package_digest,
                    left_record,
                    &safe_matches[0],
                )?);
                continue;
            }

            if safe_matches.len() > 1 {
                review_states.push(build_review_state(
                    &map,
                    ReconcileReviewStateKind::OneToMany,
                    match_value,
                    left_record,
                    safe_matches,
                    Vec::new(),
                    "declared field map matched more than one right-side record".to_string(),
                )?);
                continue;
            }

            if !stale_matches.is_empty() {
                review_states.push(build_review_state(
                    &map,
                    ReconcileReviewStateKind::StaleLink,
                    match_value,
                    left_record,
                    stale_matches,
                    Vec::new(),
                    "declared field map only matched stale right-side records".to_string(),
                )?);
                continue;
            }

            if !had_any_candidate {
                review_states.push(build_review_state(
                    &map,
                    ReconcileReviewStateKind::MissingLink,
                    match_value,
                    left_record,
                    Vec::new(),
                    Vec::new(),
                    "declared field map found no right-side match".to_string(),
                )?);
            }
        }
    }

    let mut conflict_states = conflicting_review_states(&evidence)?;
    review_states.append(&mut conflict_states);

    evidence.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
    review_states.sort_by(|left, right| left.review_id.cmp(&right.review_id));
    abstentions.sort_by(|left, right| left.abstention_id.cmp(&right.abstention_id));

    Ok(ProviderReconcileReport {
        package_digest,
        as_of: input.as_of.clone(),
        evidence,
        review_states,
        abstentions,
        registry_write_intents: Vec::new(),
    })
}

pub fn simulate_reconciliation_impact(
    report: &ProviderReconcileReport,
) -> ProviderReconcileImpactSummary {
    let proposed_identity = report
        .evidence
        .iter()
        .filter(|record| record.evidence_kind == ReconcileEvidenceKind::ProposedIdentity)
        .count();
    let proposed_relationship = report
        .evidence
        .iter()
        .filter(|record| record.evidence_kind == ReconcileEvidenceKind::ProposedRelationship)
        .count();
    let cannot_link = report
        .evidence
        .iter()
        .filter(|record| record.evidence_kind == ReconcileEvidenceKind::CannotLink)
        .count();

    ProviderReconcileImpactSummary {
        proposed_identity,
        proposed_relationship,
        cannot_link,
        review_states: report.review_states.len(),
        abstentions: report.abstentions.len(),
    }
}

fn normalize_documentation_ref(
    mut entry: ProviderReconcileDocumentationRef,
) -> ProviderReconcileResult<ProviderReconcileDocumentationRef> {
    entry.label = normalized_free_text(&entry.label, "documentation.label")?;
    entry.uri = normalized_uri(&entry.uri, "documentation.uri")?;
    Ok(entry)
}

fn normalize_field_map(
    mut map: ProviderReconcileFieldMap,
) -> ProviderReconcileResult<ProviderReconcileFieldMap> {
    map.map_id = normalized_opaque_ref(&map.map_id, "map_id")?;
    map.left_provider_id = normalized_identifier(&map.left_provider_id, "left_provider_id")?;
    map.left_namespace = normalized_opaque_ref(&map.left_namespace, "left_namespace")?;
    map.left_scope_id = normalized_opaque_ref(&map.left_scope_id, "left_scope_id")?;
    map.left_type_ref = normalized_opaque_ref(&map.left_type_ref, "left_type_ref")?;
    map.right_provider_id = normalized_identifier(&map.right_provider_id, "right_provider_id")?;
    map.right_namespace = normalized_opaque_ref(&map.right_namespace, "right_namespace")?;
    map.right_scope_id = normalized_opaque_ref(&map.right_scope_id, "right_scope_id")?;
    map.right_type_ref = normalized_opaque_ref(&map.right_type_ref, "right_type_ref")?;
    map.left_field_path = normalized_field_path(&map.left_field_path, "left_field_path")?;
    map.right_field_path = normalized_field_path(&map.right_field_path, "right_field_path")?;
    map.relationship_term_id = map
        .relationship_term_id
        .map(|value| normalized_opaque_ref(&value, "relationship_term_id"))
        .transpose()?;

    match map.evidence_kind {
        ReconcileEvidenceKind::ProposedRelationship => {
            if map.relationship_term_id.is_none() {
                return Err(artifact_contract_error(format!(
                    "relationship field map {} must declare relationship_term_id",
                    map.map_id
                )));
            }
        }
        ReconcileEvidenceKind::ProposedIdentity | ReconcileEvidenceKind::CannotLink => {
            if map.relationship_term_id.is_some() {
                return Err(artifact_contract_error(format!(
                    "non-relationship field map {} cannot declare relationship_term_id",
                    map.map_id
                )));
            }
        }
    }

    Ok(map)
}

fn finalize_map_ref(
    mut reference: ProviderReconcileMapRef,
) -> ProviderReconcileResult<ProviderReconcileMapRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.map_id = normalized_opaque_ref(&reference.map_id, "map_id")?;
    Ok(reference)
}

fn mismatch_reason(
    left_record: &ProviderNativeRecord,
    right_record: &ProviderNativeRecord,
    map: &ProviderReconcileFieldMap,
) -> Option<(ReconcileAbstentionKind, String)> {
    if left_record.native_namespace != map.left_namespace
        || right_record.native_namespace != map.right_namespace
    {
        return Some((
            ReconcileAbstentionKind::Namespace,
            format!(
                "native namespace mismatch: left {} vs {}, right {} vs {}",
                left_record.native_namespace,
                map.left_namespace,
                right_record.native_namespace,
                map.right_namespace
            ),
        ));
    }
    if left_record.scope_id != map.left_scope_id || right_record.scope_id != map.right_scope_id {
        return Some((
            ReconcileAbstentionKind::Scope,
            format!(
                "scope mismatch: left {} vs {}, right {} vs {}",
                left_record.scope_id, map.left_scope_id, right_record.scope_id, map.right_scope_id
            ),
        ));
    }
    if left_record.object_type_ref != map.left_type_ref
        || right_record.object_type_ref != map.right_type_ref
    {
        return Some((
            ReconcileAbstentionKind::Type,
            format!(
                "type mismatch: left {} vs {}, right {} vs {}",
                left_record.object_type_ref,
                map.left_type_ref,
                right_record.object_type_ref,
                map.right_type_ref
            ),
        ));
    }
    None
}

fn build_evidence(
    map: &ProviderReconcileFieldMap,
    match_value: &str,
    package_digest: &str,
    left_record: &ProviderNativeRecord,
    right_ref: &ProviderNativeRef,
) -> ProviderReconcileResult<ProviderReconcileEvidenceRecord> {
    let left = native_ref(left_record);
    let payload = (
        &map.map_id,
        map.evidence_kind,
        match_value,
        &left,
        right_ref,
        package_digest,
        &map.relationship_term_id,
    );
    Ok(ProviderReconcileEvidenceRecord {
        evidence_id: digest_id("reconcile-evidence", &payload)?,
        map_id: map.map_id.clone(),
        evidence_kind: map.evidence_kind,
        match_value: match_value.to_string(),
        relationship_term_id: map.relationship_term_id.clone(),
        package_digest: package_digest.to_string(),
        left,
        right: right_ref.clone(),
    })
}

fn build_review_state(
    map: &ProviderReconcileFieldMap,
    state_kind: ReconcileReviewStateKind,
    match_value: &str,
    left_record: &ProviderNativeRecord,
    mut right_candidates: Vec<ProviderNativeRef>,
    mut related_evidence_ids: Vec<String>,
    message: String,
) -> ProviderReconcileResult<ProviderReconcileReviewState> {
    right_candidates.sort();
    right_candidates.dedup();
    related_evidence_ids.sort();
    related_evidence_ids.dedup();

    let left = native_ref(left_record);
    let payload = (
        &map.map_id,
        state_kind,
        match_value,
        &left,
        &right_candidates,
        &related_evidence_ids,
    );

    Ok(ProviderReconcileReviewState {
        review_id: digest_id("reconcile-review", &payload)?,
        map_id: map.map_id.clone(),
        state_kind,
        match_value: match_value.to_string(),
        left,
        right_candidates,
        related_evidence_ids,
        message,
    })
}

fn build_abstention(
    map: &ProviderReconcileFieldMap,
    kind: ReconcileAbstentionKind,
    match_value: &str,
    left_record: &ProviderNativeRecord,
    right_record: &ProviderNativeRecord,
    message: String,
) -> ProviderReconcileResult<ProviderReconcileAbstention> {
    let left = native_ref(left_record);
    let right = native_ref(right_record);
    let payload = (&map.map_id, kind, match_value, &left, &right);

    Ok(ProviderReconcileAbstention {
        abstention_id: digest_id("reconcile-abstention", &payload)?,
        map_id: map.map_id.clone(),
        kind,
        match_value: match_value.to_string(),
        left,
        right,
        message,
    })
}

fn conflicting_review_states(
    evidence: &[ProviderReconcileEvidenceRecord],
) -> ProviderReconcileResult<Vec<ProviderReconcileReviewState>> {
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
    struct PairKey {
        left_provider_id: String,
        left_namespace: String,
        left_native_id: String,
        right_provider_id: String,
        right_namespace: String,
        right_native_id: String,
    }

    let mut grouped =
        std::collections::BTreeMap::<PairKey, Vec<&ProviderReconcileEvidenceRecord>>::new();
    for record in evidence {
        grouped
            .entry(PairKey {
                left_provider_id: record.left.provider_id.clone(),
                left_namespace: record.left.native_namespace.clone(),
                left_native_id: record.left.native_id.clone(),
                right_provider_id: record.right.provider_id.clone(),
                right_namespace: record.right.native_namespace.clone(),
                right_native_id: record.right.native_id.clone(),
            })
            .or_default()
            .push(record);
    }

    let mut review_states = Vec::new();
    for records in grouped.into_values() {
        let kinds = records
            .iter()
            .map(|record| record.evidence_kind)
            .collect::<BTreeSet<_>>();
        if kinds.len() < 2 {
            continue;
        }

        let first = records[0];
        let mut right_candidates = vec![first.right.clone()];
        let mut related_evidence_ids = records
            .iter()
            .map(|record| record.evidence_id.clone())
            .collect::<Vec<_>>();
        right_candidates.sort();
        right_candidates.dedup();
        related_evidence_ids.sort();
        related_evidence_ids.dedup();

        let payload = (
            &first.map_id,
            ReconcileReviewStateKind::ConflictingLink,
            &first.match_value,
            &first.left,
            &right_candidates,
            &related_evidence_ids,
        );

        review_states.push(ProviderReconcileReviewState {
            review_id: digest_id("reconcile-review", &payload)?,
            map_id: first.map_id.clone(),
            state_kind: ReconcileReviewStateKind::ConflictingLink,
            match_value: first.match_value.clone(),
            left: first.left.clone(),
            right_candidates,
            related_evidence_ids,
            message: "same pair produced more than one evidence kind and must stay reviewable"
                .to_string(),
        });
    }

    Ok(review_states)
}

fn field_value<'a>(record: &'a ProviderNativeRecord, field_path: &str) -> Option<&'a str> {
    if field_path == "native_id" {
        return Some(record.native_id.as_str());
    }
    record.fields.get(field_path).map(String::as_str)
}

fn values_match(left: &str, right: &str, comparator: ReconcileFieldComparator) -> bool {
    match comparator {
        ReconcileFieldComparator::Exact => left == right,
    }
}

fn is_stale(
    left_record: &ProviderNativeRecord,
    right_record: &ProviderNativeRecord,
    as_of: NaiveDate,
    stale_after_days: u32,
) -> ProviderReconcileResult<bool> {
    let left_observed = parse_date(&left_record.observed_at, "left.observed_at")?;
    let right_observed = parse_date(&right_record.observed_at, "right.observed_at")?;
    let left_age = (as_of - left_observed).num_days();
    let right_age = (as_of - right_observed).num_days();
    Ok(left_age > i64::from(stale_after_days) || right_age > i64::from(stale_after_days))
}

fn native_ref(record: &ProviderNativeRecord) -> ProviderNativeRef {
    ProviderNativeRef {
        provider_id: record.provider_id.clone(),
        native_namespace: record.native_namespace.clone(),
        native_id: record.native_id.clone(),
        scope_id: record.scope_id.clone(),
        object_type_ref: record.object_type_ref.clone(),
        observed_at: record.observed_at.clone(),
    }
}

fn parse_date(value: &str, field: &str) -> ProviderReconcileResult<NaiveDate> {
    let trimmed = value.trim();
    if let Ok(date_time) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(date_time.date_naive());
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|error| {
        ProviderReconcileError::new(
            ProviderReconcileErrorCode::DateParse,
            format!("failed to parse {field} as RFC3339 or YYYY-MM-DD: {error}"),
        )
    })
}

fn digest_id<T: Serialize>(prefix: &str, value: &T) -> ProviderReconcileResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize reconcile payload: {error}"))
    })?;
    Ok(format!("{prefix}:blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn normalized_identifier(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!("{field} cannot be empty")));
    }
    if trimmed.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    }) {
        return Err(artifact_contract_error(format!(
            "{field} must use only ASCII alphanumerics, '.', '_' or '-'"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalized_opaque_ref(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!("{field} cannot be empty")));
    }
    if !trimmed.contains(':') {
        return Err(artifact_contract_error(format!(
            "{field} must be an opaque namespaced ref"
        )));
    }
    if trimmed.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':'))
    }) {
        return Err(artifact_contract_error(format!(
            "{field} must use only ASCII alphanumerics, '.', '_', '-' or ':'"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalized_field_path(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!("{field} cannot be empty")));
    }
    if trimmed.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    }) {
        return Err(artifact_contract_error(format!(
            "{field} must use only ASCII alphanumerics, '.', '_' or '-'"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalized_free_text(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!("{field} cannot be empty")));
    }
    Ok(trimmed.to_string())
}

fn normalized_uri(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!("{field} cannot be empty")));
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(artifact_contract_error(format!(
            "{field} must start with http:// or https://"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalized_semver(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    let parts = trimmed.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| part.is_empty())
        || parts.iter().any(|part| part.parse::<u64>().is_err())
    {
        return Err(artifact_contract_error(format!(
            "{field} must be a semantic version like 1.2.3"
        )));
    }
    Ok(trimmed.to_string())
}

fn semver_major(value: &str) -> ProviderReconcileResult<u64> {
    value
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("semantic version is missing major component"))?
        .parse::<u64>()
        .map_err(|_| artifact_contract_error("semantic version major component must be numeric"))
}

fn normalized_hash(value: &str, field: &str) -> ProviderReconcileResult<String> {
    let trimmed = value.trim();
    if trimmed.len() != 71 || !trimmed.starts_with("blake3:") {
        return Err(artifact_contract_error(format!(
            "{field} must be a blake3 digest"
        )));
    }
    if !trimmed[7..]
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err(artifact_contract_error(format!(
            "{field} must be a lowercase hexadecimal blake3 digest"
        )));
    }
    Ok(trimmed.to_string())
}

fn artifact_contract_error(message: impl Into<String>) -> ProviderReconcileError {
    ProviderReconcileError::new(ProviderReconcileErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> ProviderReconcileError {
    ProviderReconcileError::new(ProviderReconcileErrorCode::CompatibilityPolicy, message)
}

fn missing_field_map_error(message: impl Into<String>) -> ProviderReconcileError {
    ProviderReconcileError::new(ProviderReconcileErrorCode::MissingFieldMap, message)
}
