// Temporal alias and trusted-anchor claims.
//
// The lookup compiler must preserve source-scoped alias history, interval
// validity, and promotion provenance so local claims do not silently leak into
// global exact lookup.

use super::fact::{
    AssertionStatus, IntervalBoundary, RecordedTime, SourceLocator, TemporalError,
    TemporalErrorCode, TemporalResult, TimeInterval,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CANON_TEMPORAL_ALIAS_VERSION: &str = "canon.temporal.alias.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasValueKind {
    Name,
    Identifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LookupVisibility {
    #[default]
    SourceScoped,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnchorExclusivity {
    #[default]
    Exclusive,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct AliasScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PromotionProvenance {
    pub policy_clause_id: String,
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrustedAnchor {
    pub namespace: String,
    pub value: String,
    #[serde(default)]
    pub exclusivity: AnchorExclusivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasClaim {
    pub version: String,
    pub claim_id: String,
    pub claim_key: String,
    pub conflict_key: String,
    pub alias_value: String,
    pub alias_kind: AliasValueKind,
    pub entity_id: String,
    #[serde(default)]
    pub lookup_visibility: LookupVisibility,
    #[serde(default)]
    pub scope: AliasScope,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub source_locator: SourceLocator,
    pub materialization_digest: String,
    pub assertion_status: AssertionStatus,
    pub trust_policy_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_to_global_by: Option<PromotionProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_anchor: Option<TrustedAnchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasSnapshot {
    pub valid_at: String,
    pub known_as_of: String,
    pub active_claims: Vec<AliasClaim>,
    pub history: Vec<AliasClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_claim_ids: Vec<String>,
}

pub fn finalize_alias_claim(mut claim: AliasClaim) -> TemporalResult<AliasClaim> {
    if claim.version.trim().is_empty() {
        claim.version = CANON_TEMPORAL_ALIAS_VERSION.to_string();
    }
    if claim.version != CANON_TEMPORAL_ALIAS_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported temporal alias version: {}",
            claim.version
        )));
    }

    claim.alias_value = normalized_non_empty(&claim.alias_value, "alias_value")?;
    claim.entity_id = normalized_non_empty(&claim.entity_id, "entity_id")?;
    claim.scope = normalize_scope(claim.scope)?;
    claim.valid_time = normalize_interval(claim.valid_time, "valid_time")?;
    claim.recorded_time = normalize_recorded_time(claim.recorded_time)?;
    claim.source_locator = normalize_source_locator(claim.source_locator)?;
    claim.materialization_digest =
        normalized_hash(&claim.materialization_digest, "materialization_digest")?;
    claim.trust_policy_ref = normalized_non_empty(&claim.trust_policy_ref, "trust_policy_ref")?;
    claim.promoted_to_global_by = claim
        .promoted_to_global_by
        .map(normalize_promotion)
        .transpose()?;
    claim.trusted_anchor = claim
        .trusted_anchor
        .map(normalize_trusted_anchor)
        .transpose()?;

    normalize_link_ids(&mut claim.supersedes, "supersedes")?;
    normalize_link_ids(&mut claim.retracts, "retracts")?;
    if overlaps(&claim.supersedes, &claim.retracts) {
        return Err(link_invariant_error(
            "supersedes and retracts cannot reference the same claim_id",
        ));
    }
    validate_status_links(&claim)?;
    validate_visibility_rules(&claim)?;

    claim.claim_key = compute_claim_key(&claim)?;
    claim.conflict_key = compute_conflict_key(&claim)?;
    claim.claim_id.clear();
    claim.claim_id = compute_claim_id(&claim)?;
    Ok(claim)
}

pub fn finalize_alias_claims(
    claims: impl IntoIterator<Item = AliasClaim>,
) -> TemporalResult<Vec<AliasClaim>> {
    let mut normalized = claims
        .into_iter()
        .map(finalize_alias_claim)
        .collect::<TemporalResult<Vec<_>>>()?;
    normalized.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));

    let mut deduped: Vec<AliasClaim> = Vec::with_capacity(normalized.len());
    for claim in normalized {
        if let Some(previous) = deduped.last()
            && previous.claim_id == claim.claim_id
        {
            if previous != &claim {
                return Err(link_invariant_error(
                    "non-identical alias claims collided on the same claim_id",
                ));
            }
            continue;
        }
        deduped.push(claim);
    }
    Ok(deduped)
}

pub fn compile_alias_snapshot(
    claims: &[AliasClaim],
    valid_at: &str,
    known_as_of: &str,
) -> TemporalResult<AliasSnapshot> {
    let valid_at = canonical_timestamp(valid_at, "valid_at")?;
    let known_as_of = canonical_timestamp(known_as_of, "known_as_of")?;
    let history = finalize_alias_claims(claims.to_vec())?
        .into_iter()
        .filter(|claim| recorded_contains(&claim.recorded_time, &known_as_of))
        .collect::<Vec<_>>();

    let mut suppressed = BTreeSet::new();
    for claim in &history {
        suppressed.extend(claim.supersedes.iter().cloned());
        if matches!(claim.assertion_status, AssertionStatus::Retracted) {
            suppressed.extend(claim.retracts.iter().cloned());
        }
    }

    let active_claims = history
        .iter()
        .filter(|claim| !suppressed.contains(&claim.claim_id))
        .filter(|claim| is_snapshot_candidate(claim.assertion_status))
        .filter(|claim| interval_contains(&claim.valid_time, &valid_at))
        .cloned()
        .collect::<Vec<_>>();

    Ok(AliasSnapshot {
        valid_at,
        known_as_of,
        active_claims,
        history,
        suppressed_claim_ids: suppressed.into_iter().collect(),
    })
}

pub fn global_exact_lookup_claims(snapshot: &AliasSnapshot) -> Vec<AliasClaim> {
    snapshot
        .active_claims
        .iter()
        .filter(|claim| matches!(claim.lookup_visibility, LookupVisibility::Global))
        .cloned()
        .collect()
}

pub fn source_exact_lookup_claims(
    snapshot: &AliasSnapshot,
    source_system: &str,
    scope_type: Option<&str>,
    scope_id: Option<&str>,
) -> TemporalResult<Vec<AliasClaim>> {
    let source_system = normalized_non_empty(source_system, "source_system")?;
    let scope_type = scope_type
        .map(|value| normalized_non_empty(value, "scope_type"))
        .transpose()?;
    let scope_id = scope_id
        .map(|value| normalized_non_empty(value, "scope_id"))
        .transpose()?;

    if scope_type.is_some() ^ scope_id.is_some() {
        return Err(artifact_contract_error(
            "scope_type and scope_id must both be set or both be omitted",
        ));
    }

    let mut visible = Vec::new();
    for claim in &snapshot.active_claims {
        if matches!(claim.lookup_visibility, LookupVisibility::Global) {
            visible.push(claim.clone());
            continue;
        }

        if claim.scope.source_system.as_deref() != Some(source_system.as_str()) {
            continue;
        }
        if claim.scope.scope_type.as_deref() != scope_type.as_deref() {
            continue;
        }
        if claim.scope.scope_id.as_deref() != scope_id.as_deref() {
            continue;
        }
        visible.push(claim.clone());
    }
    visible.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
    Ok(visible)
}

pub fn alias_lookup_key(claim: &AliasClaim) -> String {
    claim.conflict_key.clone()
}

pub fn trusted_anchor_key(claim: &AliasClaim) -> Option<String> {
    claim
        .trusted_anchor
        .as_ref()
        .map(|anchor| format!("{}:{}", anchor.namespace, anchor.value))
}

pub fn intervals_overlap(left: &TimeInterval, right: &TimeInterval) -> bool {
    !interval_precedes(left, right) && !interval_precedes(right, left)
}

pub fn interval_contains(interval: &TimeInterval, at: &str) -> bool {
    if let Some(start_at) = interval.start_at.as_deref() {
        if at < start_at {
            return false;
        }
        if at == start_at && matches!(interval.start_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    if let Some(end_at) = interval.end_at.as_deref() {
        if at > end_at {
            return false;
        }
        if at == end_at && matches!(interval.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
}

pub fn recorded_contains(recorded_time: &RecordedTime, known_as_of: &str) -> bool {
    if let Some(start_at) = recorded_time.start_at.as_deref() {
        if known_as_of < start_at {
            return false;
        }
        if known_as_of == start_at
            && matches!(recorded_time.start_bound, IntervalBoundary::Exclusive)
        {
            return false;
        }
    }
    if let Some(end_at) = recorded_time.end_at.as_deref() {
        if known_as_of > end_at {
            return false;
        }
        if known_as_of == end_at && matches!(recorded_time.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
}

fn is_snapshot_candidate(status: AssertionStatus) -> bool {
    matches!(
        status,
        AssertionStatus::Asserted | AssertionStatus::Accepted
    )
}

fn interval_precedes(left: &TimeInterval, right: &TimeInterval) -> bool {
    let Some(left_end) = left.end_at.as_deref() else {
        return false;
    };
    let Some(right_start) = right.start_at.as_deref() else {
        return false;
    };
    if left_end < right_start {
        return true;
    }
    left_end == right_start
        && (matches!(left.end_bound, IntervalBoundary::Exclusive)
            || matches!(right.start_bound, IntervalBoundary::Exclusive))
}

fn validate_visibility_rules(claim: &AliasClaim) -> TemporalResult<()> {
    match claim.lookup_visibility {
        LookupVisibility::SourceScoped => {
            if claim.scope.source_system.is_none() {
                return Err(artifact_contract_error(
                    "source-scoped aliases require scope.source_system",
                ));
            }
            if claim.promoted_to_global_by.is_some() {
                return Err(artifact_contract_error(
                    "source-scoped aliases cannot carry promoted_to_global_by",
                ));
            }
        }
        LookupVisibility::Global => {
            if claim.scope.source_system.is_some() && claim.promoted_to_global_by.is_none() {
                return Err(artifact_contract_error(
                    "source-local aliases require promoted_to_global_by before global lookup",
                ));
            }
        }
    }
    Ok(())
}

fn normalize_interval(mut interval: TimeInterval, field: &str) -> TemporalResult<TimeInterval> {
    interval.start_at = canonical_optional_timestamp(interval.start_at.take(), field, "start_at")?;
    interval.end_at = canonical_optional_timestamp(interval.end_at.take(), field, "end_at")?;

    if interval.start_at.is_none() {
        interval.start_bound = IntervalBoundary::Open;
    } else if matches!(interval.start_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(format!(
            "{field}.start_bound cannot be open when {field}.start_at is present"
        )));
    }

    if interval.end_at.is_none() {
        interval.end_bound = IntervalBoundary::Open;
    } else if matches!(interval.end_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(format!(
            "{field}.end_bound cannot be open when {field}.end_at is present"
        )));
    }

    validate_interval_bounds(
        interval.start_at.as_deref(),
        interval.start_bound,
        interval.end_at.as_deref(),
        interval.end_bound,
        field,
    )?;
    Ok(interval)
}

fn normalize_recorded_time(mut recorded_time: RecordedTime) -> TemporalResult<RecordedTime> {
    recorded_time.start_at =
        canonical_optional_timestamp(recorded_time.start_at.take(), "recorded_time", "start_at")?;
    recorded_time.end_at =
        canonical_optional_timestamp(recorded_time.end_at.take(), "recorded_time", "end_at")?;

    if recorded_time.start_at.is_none() {
        recorded_time.start_bound = IntervalBoundary::Open;
    } else if matches!(recorded_time.start_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(
            "recorded_time.start_bound cannot be open when recorded_time.start_at is present",
        ));
    }

    if recorded_time.end_at.is_none() {
        recorded_time.end_bound = IntervalBoundary::Open;
    } else if matches!(recorded_time.end_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(
            "recorded_time.end_bound cannot be open when recorded_time.end_at is present",
        ));
    }

    let has_interval = recorded_time.start_at.is_some() || recorded_time.end_at.is_some();
    if !has_interval && recorded_time.transaction_seq.is_none() {
        return Err(artifact_contract_error(
            "recorded_time requires an interval bound, a transaction_seq, or both",
        ));
    }

    validate_interval_bounds(
        recorded_time.start_at.as_deref(),
        recorded_time.start_bound,
        recorded_time.end_at.as_deref(),
        recorded_time.end_bound,
        "recorded_time",
    )?;
    Ok(recorded_time)
}

fn validate_interval_bounds(
    start_at: Option<&str>,
    start_bound: IntervalBoundary,
    end_at: Option<&str>,
    end_bound: IntervalBoundary,
    field: &str,
) -> TemporalResult<()> {
    if let (Some(start_at), Some(end_at)) = (start_at, end_at) {
        if start_at > end_at {
            return Err(artifact_contract_error(format!(
                "{field}.start_at must be <= {field}.end_at"
            )));
        }
        if start_at == end_at
            && (!matches!(start_bound, IntervalBoundary::Inclusive)
                || !matches!(end_bound, IntervalBoundary::Inclusive))
        {
            return Err(artifact_contract_error(format!(
                "{field} cannot be empty when start_at == end_at"
            )));
        }
    }
    Ok(())
}

fn normalize_source_locator(mut locator: SourceLocator) -> TemporalResult<SourceLocator> {
    locator.source_system =
        normalized_non_empty(&locator.source_system, "source_locator.source_system")?;
    locator.locator = normalized_non_empty(&locator.locator, "source_locator.locator")?;
    locator.fragment = locator
        .fragment
        .take()
        .map(|fragment| fragment.trim().to_string())
        .filter(|fragment| !fragment.is_empty());
    Ok(locator)
}

fn normalize_scope(mut scope: AliasScope) -> TemporalResult<AliasScope> {
    scope.source_system = scope
        .source_system
        .take()
        .map(|value| normalized_non_empty(&value, "scope.source_system"))
        .transpose()?;
    scope.scope_type = scope
        .scope_type
        .take()
        .map(|value| normalized_non_empty(&value, "scope.scope_type"))
        .transpose()?;
    scope.scope_id = scope
        .scope_id
        .take()
        .map(|value| normalized_non_empty(&value, "scope.scope_id"))
        .transpose()?;
    if scope.scope_type.is_some() ^ scope.scope_id.is_some() {
        return Err(artifact_contract_error(
            "scope.scope_type and scope.scope_id must both be set or both be omitted",
        ));
    }
    Ok(scope)
}

fn normalize_promotion(mut promotion: PromotionProvenance) -> TemporalResult<PromotionProvenance> {
    promotion.policy_clause_id = normalized_non_empty(
        &promotion.policy_clause_id,
        "promoted_to_global_by.policy_clause_id",
    )?;
    promotion.evidence_ref = normalized_non_empty(
        &promotion.evidence_ref,
        "promoted_to_global_by.evidence_ref",
    )?;
    Ok(promotion)
}

fn normalize_trusted_anchor(mut anchor: TrustedAnchor) -> TemporalResult<TrustedAnchor> {
    anchor.namespace = normalized_non_empty(&anchor.namespace, "trusted_anchor.namespace")?;
    anchor.value = normalized_non_empty(&anchor.value, "trusted_anchor.value")?;
    Ok(anchor)
}

fn normalize_link_ids(link_ids: &mut Vec<String>, field: &str) -> TemporalResult<()> {
    let mut normalized = Vec::with_capacity(link_ids.len());
    for link_id in link_ids.drain(..) {
        normalized.push(normalized_hash(&link_id, field)?);
    }
    normalized.sort();
    normalized.dedup();
    *link_ids = normalized;
    Ok(())
}

fn validate_status_links(claim: &AliasClaim) -> TemporalResult<()> {
    if matches!(claim.assertion_status, AssertionStatus::Retracted) && claim.retracts.is_empty() {
        return Err(link_invariant_error(
            "retracted alias claims require at least one retracts link",
        ));
    }
    if matches!(claim.assertion_status, AssertionStatus::Superseded) && claim.supersedes.is_empty()
    {
        return Err(link_invariant_error(
            "superseded alias claims require at least one supersedes link",
        ));
    }
    Ok(())
}

fn compute_claim_key(claim: &AliasClaim) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ClaimKey<'a> {
        version: &'static str,
        alias_value: &'a str,
        alias_kind: AliasValueKind,
        entity_id: &'a str,
        lookup_visibility: LookupVisibility,
        scope: &'a AliasScope,
        valid_time: &'a TimeInterval,
        trusted_anchor: &'a Option<TrustedAnchor>,
    }

    hash_struct(
        &ClaimKey {
            version: CANON_TEMPORAL_ALIAS_VERSION,
            alias_value: &claim.alias_value,
            alias_kind: claim.alias_kind,
            entity_id: &claim.entity_id,
            lookup_visibility: claim.lookup_visibility,
            scope: &claim.scope,
            valid_time: &claim.valid_time,
            trusted_anchor: &claim.trusted_anchor,
        },
        "claim_key",
    )
}

fn compute_conflict_key(claim: &AliasClaim) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ConflictKey<'a> {
        version: &'static str,
        alias_value: &'a str,
        alias_kind: AliasValueKind,
        lookup_visibility: LookupVisibility,
        scope: &'a AliasScope,
        trusted_anchor: &'a Option<TrustedAnchor>,
    }

    hash_struct(
        &ConflictKey {
            version: CANON_TEMPORAL_ALIAS_VERSION,
            alias_value: &claim.alias_value,
            alias_kind: claim.alias_kind,
            lookup_visibility: claim.lookup_visibility,
            scope: &claim.scope,
            trusted_anchor: &claim.trusted_anchor,
        },
        "conflict_key",
    )
}

fn compute_claim_id(claim: &AliasClaim) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ClaimId<'a> {
        version: &'static str,
        claim_key: &'a str,
        recorded_time: &'a RecordedTime,
        source_locator: &'a SourceLocator,
        materialization_digest: &'a str,
        assertion_status: AssertionStatus,
        trust_policy_ref: &'a str,
        promoted_to_global_by: &'a Option<PromotionProvenance>,
        supersedes: &'a [String],
        retracts: &'a [String],
    }

    hash_struct(
        &ClaimId {
            version: CANON_TEMPORAL_ALIAS_VERSION,
            claim_key: &claim.claim_key,
            recorded_time: &claim.recorded_time,
            source_locator: &claim.source_locator,
            materialization_digest: &claim.materialization_digest,
            assertion_status: claim.assertion_status,
            trust_policy_ref: &claim.trust_policy_ref,
            promoted_to_global_by: &claim.promoted_to_global_by,
            supersedes: &claim.supersedes,
            retracts: &claim.retracts,
        },
        "claim_id",
    )
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

fn canonical_optional_timestamp(
    value: Option<String>,
    field: &str,
    leaf: &str,
) -> TemporalResult<Option<String>> {
    value
        .map(|value| canonical_timestamp(&value, &format!("{field}.{leaf}")))
        .transpose()
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
    let Some(hex) = normalized.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    }
    Ok(normalized)
}

fn hash_struct<T: Serialize>(value: &T, label: &str) -> TemporalResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {label}: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn overlaps(left: &[String], right: &[String]) -> bool {
    let left = left.iter().collect::<BTreeSet<_>>();
    right.iter().any(|value| left.contains(&value))
}

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::ArtifactContract, message)
}

fn link_invariant_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::LinkInvariant, message)
}
