#![forbid(unsafe_code)]

//! Deterministic bitemporal identity assertions.
//!
//! These facts are provenance-carrying assertions, not automatically trusted
//! truth. They preserve validity time, knowledge time, and correction links
//! without changing exact runtime lookup semantics.

pub mod diff;
pub mod explain;

mod fact;

pub use fact::*;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

pub fn finalize_fact(mut fact: IdentityFact) -> TemporalResult<IdentityFact> {
    if fact.version.trim().is_empty() {
        fact.version = CANON_IDENTITY_FACT_VERSION.to_string();
    }
    if fact.version != CANON_IDENTITY_FACT_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported temporal fact version: {}",
            fact.version
        )));
    }

    fact.subject_id = normalized_non_empty(&fact.subject_id, "subject_id")?;
    fact.predicate = normalized_non_empty(&fact.predicate, "predicate")?;
    fact.object_id = normalized_non_empty(&fact.object_id, "object_id")?;
    fact.trust_policy_ref = normalized_non_empty(&fact.trust_policy_ref, "trust_policy_ref")?;
    fact.valid_time = normalize_interval(fact.valid_time, "valid_time")?;
    fact.recorded_time = normalize_recorded_time(fact.recorded_time)?;
    fact.source_locator = normalize_source_locator(fact.source_locator)?;
    fact.materialization_digest =
        normalized_hash(&fact.materialization_digest, "materialization_digest")?;
    fact.scope = fact.scope.map(normalize_scope).transpose()?;

    normalize_link_ids(&mut fact.supersedes, "supersedes")?;
    normalize_link_ids(&mut fact.retracts, "retracts")?;
    if overlaps(&fact.supersedes, &fact.retracts) {
        return Err(link_invariant_error(
            "supersedes and retracts cannot reference the same fact_id",
        ));
    }
    validate_status_links(&fact)?;

    fact.assertion_key = compute_assertion_key(&fact)?;
    fact.conflict_key = compute_conflict_key(&fact)?;
    fact.fact_id.clear();
    fact.fact_id = compute_fact_id(&fact)?;
    Ok(fact)
}

pub fn finalize_facts(
    facts: impl IntoIterator<Item = IdentityFact>,
) -> TemporalResult<Vec<IdentityFact>> {
    let mut normalized = Vec::new();
    for fact in facts {
        normalized.push(finalize_fact(fact)?);
    }
    normalized.sort_by(fact_cmp);

    let mut deduped: Vec<IdentityFact> = Vec::with_capacity(normalized.len());
    for fact in normalized {
        if let Some(last) = deduped.last()
            && last.fact_id == fact.fact_id
        {
            if last != &fact {
                return Err(link_invariant_error(
                    "non-identical facts collided on the same fact_id",
                ));
            }
            continue;
        }
        deduped.push(fact);
    }

    Ok(deduped)
}

pub fn canonical_json_bytes(fact: &IdentityFact) -> TemporalResult<Vec<u8>> {
    let fact = finalize_fact(fact.clone())?;
    serde_json::to_vec(&fact).map_err(|error| {
        artifact_contract_error(format!("failed to serialize temporal fact: {error}"))
    })
}

pub fn canonical_fact_set_bytes(facts: &[IdentityFact]) -> TemporalResult<Vec<u8>> {
    let facts = finalize_facts(facts.to_vec())?;
    serde_json::to_vec(&facts).map_err(|error| {
        artifact_contract_error(format!("failed to serialize temporal fact set: {error}"))
    })
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

fn normalize_scope(mut scope: FactScope) -> TemporalResult<FactScope> {
    scope.scope_type = normalized_non_empty(&scope.scope_type, "scope.scope_type")?;
    scope.scope_id = normalized_non_empty(&scope.scope_id, "scope.scope_id")?;
    Ok(scope)
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

fn validate_status_links(fact: &IdentityFact) -> TemporalResult<()> {
    if matches!(fact.assertion_status, AssertionStatus::Retracted) && fact.retracts.is_empty() {
        return Err(link_invariant_error(
            "retracted facts require at least one retracts link",
        ));
    }
    if matches!(fact.assertion_status, AssertionStatus::Superseded) && fact.supersedes.is_empty() {
        return Err(link_invariant_error(
            "superseded facts require at least one supersedes link",
        ));
    }
    Ok(())
}

fn compute_assertion_key(fact: &IdentityFact) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct AssertionKey<'a> {
        version: &'static str,
        subject_id: &'a str,
        predicate: &'a str,
        object_id: &'a str,
        valid_time: &'a TimeInterval,
        scope: &'a Option<FactScope>,
    }

    let key = AssertionKey {
        version: CANON_IDENTITY_FACT_VERSION,
        subject_id: &fact.subject_id,
        predicate: &fact.predicate,
        object_id: &fact.object_id,
        valid_time: &fact.valid_time,
        scope: &fact.scope,
    };
    hash_struct(&key, "assertion_key")
}

fn compute_conflict_key(fact: &IdentityFact) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ConflictKey<'a> {
        version: &'static str,
        subject_id: &'a str,
        predicate: &'a str,
        valid_time: &'a TimeInterval,
        scope: &'a Option<FactScope>,
    }

    let key = ConflictKey {
        version: CANON_IDENTITY_FACT_VERSION,
        subject_id: &fact.subject_id,
        predicate: &fact.predicate,
        valid_time: &fact.valid_time,
        scope: &fact.scope,
    };
    hash_struct(&key, "conflict_key")
}

fn compute_fact_id(fact: &IdentityFact) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct FactId<'a> {
        version: &'static str,
        assertion_key: &'a str,
        recorded_time: &'a RecordedTime,
        source_locator: &'a SourceLocator,
        materialization_digest: &'a str,
        assertion_status: &'a AssertionStatus,
        trust_policy_ref: &'a str,
        supersedes: &'a [String],
        retracts: &'a [String],
    }

    let key = FactId {
        version: CANON_IDENTITY_FACT_VERSION,
        assertion_key: &fact.assertion_key,
        recorded_time: &fact.recorded_time,
        source_locator: &fact.source_locator,
        materialization_digest: &fact.materialization_digest,
        assertion_status: &fact.assertion_status,
        trust_policy_ref: &fact.trust_policy_ref,
        supersedes: &fact.supersedes,
        retracts: &fact.retracts,
    };
    hash_struct(&key, "fact_id")
}

fn hash_struct(value: &impl Serialize, label: &str) -> TemporalResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {label}: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn canonical_optional_timestamp(
    value: Option<String>,
    field: &str,
    part: &str,
) -> TemporalResult<Option<String>> {
    value
        .map(|value| canonical_timestamp(&value, &format!("{field}.{part}")))
        .transpose()
}

fn canonical_timestamp(value: &str, field: &str) -> TemporalResult<String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|error| {
            artifact_contract_error(format!("invalid RFC3339 timestamp for {field}: {error}"))
        })
}

fn normalized_non_empty(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} requires a non-empty string"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = value.trim();
    let Some((algorithm, digest)) = normalized.split_once(':') else {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 hash"
        )));
    };
    if !algorithm.eq_ignore_ascii_case("blake3")
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(corrupt_reference_error(format!(
            "{field} must be a canonical blake3 digest"
        )));
    }
    Ok(format!("blake3:{}", digest.to_ascii_lowercase()))
}

fn overlaps(left: &[String], right: &[String]) -> bool {
    let mut left_ix = 0;
    let mut right_ix = 0;
    while left_ix < left.len() && right_ix < right.len() {
        match left[left_ix].cmp(&right[right_ix]) {
            std::cmp::Ordering::Less => left_ix += 1,
            std::cmp::Ordering::Greater => right_ix += 1,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

fn fact_cmp(left: &IdentityFact, right: &IdentityFact) -> std::cmp::Ordering {
    left.conflict_key
        .cmp(&right.conflict_key)
        .then_with(|| left.assertion_key.cmp(&right.assertion_key))
        .then_with(|| {
            left.recorded_time
                .transaction_seq
                .cmp(&right.recorded_time.transaction_seq)
        })
        .then_with(|| {
            left.recorded_time
                .start_at
                .cmp(&right.recorded_time.start_at)
        })
        .then_with(|| {
            left.recorded_time
                .start_bound
                .cmp(&right.recorded_time.start_bound)
        })
        .then_with(|| left.recorded_time.end_at.cmp(&right.recorded_time.end_at))
        .then_with(|| {
            left.recorded_time
                .end_bound
                .cmp(&right.recorded_time.end_bound)
        })
        .then_with(|| {
            left.source_locator
                .source_system
                .cmp(&right.source_locator.source_system)
        })
        .then_with(|| {
            left.source_locator
                .locator
                .cmp(&right.source_locator.locator)
        })
        .then_with(|| {
            left.materialization_digest
                .cmp(&right.materialization_digest)
        })
        .then_with(|| left.fact_id.cmp(&right.fact_id))
}

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::ArtifactContract, message)
}

fn corrupt_reference_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::CorruptReference, message)
}

fn link_invariant_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::LinkInvariant, message)
}
