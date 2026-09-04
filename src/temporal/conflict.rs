// Deterministic conflict classification for temporal aliases and anchors.

use super::alias::{
    AliasClaim, AliasSnapshot, LookupVisibility, alias_lookup_key, compile_alias_snapshot,
    global_exact_lookup_claims, intervals_overlap, trusted_anchor_key,
};
use super::fact::{AssertionStatus, TemporalError, TemporalErrorCode, TemporalResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_TEMPORAL_CONFLICT_POLICY_VERSION: &str = "canon.temporal.conflict_policy.v1";
pub const CANON_TEMPORAL_CONFLICT_ARTIFACT_VERSION: &str = "canon.temporal.conflict_artifact.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictClass {
    OverlappingExclusiveAnchor,
    AliasToMultipleEntityClaim,
    SourceDisagreement,
    RecycledIdentifier,
    Retraction,
    Correction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictDisposition {
    Abstain,
    Resolved,
    HistoricalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum ConflictResolution {
    Abstain,
    PreferSourceOrder { source_systems: Vec<String> },
    PreferMostRecentCorrection,
    AllowHistoricalReuse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPolicyClause {
    pub clause_id: String,
    pub conflict_class: ConflictClass,
    pub resolution: ConflictResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPolicy {
    pub version: String,
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clauses: Vec<ConflictPolicyClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub conflict_id: String,
    pub class: ConflictClass,
    pub subject_key: String,
    pub claim_ids: Vec<String>,
    pub entity_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_clause_ids_used: Vec<String>,
    pub disposition: ConflictDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winning_claim_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictArtifact {
    pub version: String,
    pub policy_id: String,
    pub valid_at: String,
    pub known_as_of: String,
    pub claims: Vec<AliasClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_claim_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub global_exact_claim_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_clause_ids_used: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ConflictRecord>,
}

pub fn finalize_conflict_policy(mut policy: ConflictPolicy) -> TemporalResult<ConflictPolicy> {
    if policy.version.trim().is_empty() {
        policy.version = CANON_TEMPORAL_CONFLICT_POLICY_VERSION.to_string();
    }
    if policy.version != CANON_TEMPORAL_CONFLICT_POLICY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported conflict policy version: {}",
            policy.version
        )));
    }
    policy.policy_id = normalized_non_empty(&policy.policy_id, "policy_id")?;

    let mut seen_clause_ids = BTreeSet::new();
    let mut seen_classes = BTreeSet::new();
    for clause in &mut policy.clauses {
        clause.clause_id = normalized_non_empty(&clause.clause_id, "clauses.clause_id")?;
        if !seen_clause_ids.insert(clause.clause_id.clone()) {
            return Err(artifact_contract_error(format!(
                "duplicate conflict policy clause_id: {}",
                clause.clause_id
            )));
        }
        if !seen_classes.insert(clause.conflict_class) {
            return Err(artifact_contract_error(format!(
                "duplicate conflict policy class: {:?}",
                clause.conflict_class
            )));
        }
        normalize_resolution(&mut clause.resolution)?;
    }
    policy.clauses.sort_by_key(|clause| clause.conflict_class);
    Ok(policy)
}

pub fn compile_conflict_artifact(
    claims: &[AliasClaim],
    policy: ConflictPolicy,
    valid_at: &str,
    known_as_of: &str,
) -> TemporalResult<ConflictArtifact> {
    let policy = finalize_conflict_policy(policy)?;
    let snapshot = compile_alias_snapshot(claims, valid_at, known_as_of)?;
    let mut conflicts = Vec::new();
    let mut used_policy_clause_ids = BTreeSet::new();
    let mut blocked_global_claim_ids = BTreeSet::new();
    let mut winning_global_claim_ids = global_exact_lookup_claims(&snapshot)
        .into_iter()
        .map(|claim| claim.claim_id)
        .collect::<BTreeSet<_>>();

    conflicts.extend(classify_history_events(
        &snapshot,
        ConflictClass::Retraction,
        |claim| matches!(claim.assertion_status, AssertionStatus::Retracted),
    )?);
    conflicts.extend(classify_history_events(
        &snapshot,
        ConflictClass::Correction,
        |claim| !claim.supersedes.is_empty(),
    )?);
    conflicts.extend(classify_recycled_identifiers(
        &snapshot,
        &policy,
        &mut used_policy_clause_ids,
    )?);

    for record in classify_active_alias_conflicts(&snapshot, &policy, &mut used_policy_clause_ids)?
    {
        update_global_visibility(
            &record,
            &snapshot,
            &mut blocked_global_claim_ids,
            &mut winning_global_claim_ids,
        );
        conflicts.push(record);
    }
    for record in classify_active_anchor_conflicts(&snapshot, &policy, &mut used_policy_clause_ids)?
    {
        update_global_visibility(
            &record,
            &snapshot,
            &mut blocked_global_claim_ids,
            &mut winning_global_claim_ids,
        );
        conflicts.push(record);
    }

    for claim_id in blocked_global_claim_ids {
        winning_global_claim_ids.remove(&claim_id);
    }

    let mut active_claim_ids = snapshot
        .active_claims
        .iter()
        .map(|claim| claim.claim_id.clone())
        .collect::<Vec<_>>();
    active_claim_ids.sort();

    let mut claims = snapshot.history.clone();
    claims.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
    conflicts.sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));

    Ok(ConflictArtifact {
        version: CANON_TEMPORAL_CONFLICT_ARTIFACT_VERSION.to_string(),
        policy_id: policy.policy_id,
        valid_at: snapshot.valid_at,
        known_as_of: snapshot.known_as_of,
        claims,
        active_claim_ids,
        global_exact_claim_ids: winning_global_claim_ids.into_iter().collect(),
        policy_clause_ids_used: used_policy_clause_ids.into_iter().collect(),
        conflicts,
    })
}

fn classify_history_events<F>(
    snapshot: &AliasSnapshot,
    class: ConflictClass,
    predicate: F,
) -> TemporalResult<Vec<ConflictRecord>>
where
    F: Fn(&AliasClaim) -> bool,
{
    let mut records = Vec::new();
    for claim in &snapshot.history {
        if !predicate(claim) {
            continue;
        }
        let mut claim_ids = vec![claim.claim_id.clone()];
        claim_ids.extend(claim.supersedes.iter().cloned());
        claim_ids.extend(claim.retracts.iter().cloned());
        claim_ids.sort();
        claim_ids.dedup();

        let mut entity_ids = vec![claim.entity_id.clone()];
        entity_ids.sort();
        entity_ids.dedup();
        let subject_key = history_subject_key(class, claim);
        records.push(finalize_record(ConflictRecord {
            conflict_id: String::new(),
            class,
            subject_key,
            claim_ids,
            entity_ids,
            policy_clause_ids_used: Vec::new(),
            disposition: ConflictDisposition::HistoricalOnly,
            winning_claim_id: None,
            message: history_message(class),
        })?);
    }
    Ok(records)
}

fn classify_recycled_identifiers(
    snapshot: &AliasSnapshot,
    policy: &ConflictPolicy,
    used_policy_clause_ids: &mut BTreeSet<String>,
) -> TemporalResult<Vec<ConflictRecord>> {
    let mut by_anchor = BTreeMap::<String, Vec<&AliasClaim>>::new();
    for claim in &snapshot.history {
        if !matches!(
            claim.assertion_status,
            AssertionStatus::Asserted | AssertionStatus::Accepted
        ) {
            continue;
        }
        let Some(anchor_key) = trusted_anchor_key(claim) else {
            continue;
        };
        by_anchor.entry(anchor_key).or_default().push(claim);
    }

    let mut records = Vec::new();
    for (subject_key, claims) in by_anchor {
        let entity_ids = claims
            .iter()
            .map(|claim| claim.entity_id.clone())
            .collect::<BTreeSet<_>>();
        if entity_ids.len() < 2 {
            continue;
        }

        let mut saw_overlap = false;
        for index in 0..claims.len() {
            for other in claims.iter().skip(index + 1) {
                if claims[index].entity_id == other.entity_id {
                    continue;
                }
                if intervals_overlap(&claims[index].valid_time, &other.valid_time) {
                    saw_overlap = true;
                }
            }
        }
        if saw_overlap {
            continue;
        }

        let clause = find_clause(policy, ConflictClass::RecycledIdentifier);
        if let Some(clause) = clause {
            used_policy_clause_ids.insert(clause.clause_id.clone());
        }
        let message = if clause.is_some() {
            "identifier reuse remains historical and policy-scoped".to_string()
        } else {
            "identifier reuse is historical and must stay interval-scoped".to_string()
        };
        records.push(finalize_record(ConflictRecord {
            conflict_id: String::new(),
            class: ConflictClass::RecycledIdentifier,
            subject_key,
            claim_ids: sorted_claim_ids(&claims),
            entity_ids: entity_ids.into_iter().collect(),
            policy_clause_ids_used: clause
                .map(|clause| vec![clause.clause_id.clone()])
                .unwrap_or_default(),
            disposition: ConflictDisposition::HistoricalOnly,
            winning_claim_id: None,
            message,
        })?);
    }
    Ok(records)
}

fn classify_active_alias_conflicts(
    snapshot: &AliasSnapshot,
    policy: &ConflictPolicy,
    used_policy_clause_ids: &mut BTreeSet<String>,
) -> TemporalResult<Vec<ConflictRecord>> {
    let mut by_alias = BTreeMap::<String, Vec<&AliasClaim>>::new();
    for claim in &snapshot.active_claims {
        by_alias
            .entry(alias_lookup_key(claim))
            .or_default()
            .push(claim);
    }

    let mut records = Vec::new();
    for (subject_key, claims) in by_alias {
        let entity_ids = claims
            .iter()
            .map(|claim| claim.entity_id.clone())
            .collect::<BTreeSet<_>>();
        if entity_ids.len() < 2 {
            continue;
        }

        let class = if claims
            .iter()
            .map(|claim| claim.source_locator.source_system.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            ConflictClass::SourceDisagreement
        } else {
            ConflictClass::AliasToMultipleEntityClaim
        };
        let record = resolve_active_conflict(
            class,
            subject_key,
            &claims,
            policy,
            used_policy_clause_ids,
            active_message(class),
        )?;
        records.push(record);
    }
    Ok(records)
}

fn classify_active_anchor_conflicts(
    snapshot: &AliasSnapshot,
    policy: &ConflictPolicy,
    used_policy_clause_ids: &mut BTreeSet<String>,
) -> TemporalResult<Vec<ConflictRecord>> {
    let mut by_anchor = BTreeMap::<String, Vec<&AliasClaim>>::new();
    for claim in &snapshot.active_claims {
        let Some(anchor) = claim.trusted_anchor.as_ref() else {
            continue;
        };
        if !matches!(
            anchor.exclusivity,
            super::alias::AnchorExclusivity::Exclusive
        ) {
            continue;
        }
        by_anchor
            .entry(format!("{}:{}", anchor.namespace, anchor.value))
            .or_default()
            .push(claim);
    }

    let mut records = Vec::new();
    for (subject_key, claims) in by_anchor {
        let entity_ids = claims
            .iter()
            .map(|claim| claim.entity_id.clone())
            .collect::<BTreeSet<_>>();
        if entity_ids.len() < 2 {
            continue;
        }
        records.push(resolve_active_conflict(
            ConflictClass::OverlappingExclusiveAnchor,
            subject_key,
            &claims,
            policy,
            used_policy_clause_ids,
            active_message(ConflictClass::OverlappingExclusiveAnchor),
        )?);
    }
    Ok(records)
}

fn resolve_active_conflict(
    class: ConflictClass,
    subject_key: String,
    claims: &[&AliasClaim],
    policy: &ConflictPolicy,
    used_policy_clause_ids: &mut BTreeSet<String>,
    default_message: &'static str,
) -> TemporalResult<ConflictRecord> {
    let clause = find_clause(policy, class);
    let (disposition, winning_claim_id, clause_ids_used, message) =
        apply_resolution(class, claims, clause, default_message);
    for clause_id in &clause_ids_used {
        used_policy_clause_ids.insert(clause_id.clone());
    }

    finalize_record(ConflictRecord {
        conflict_id: String::new(),
        class,
        subject_key,
        claim_ids: sorted_claim_ids(claims),
        entity_ids: claims
            .iter()
            .map(|claim| claim.entity_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        policy_clause_ids_used: clause_ids_used,
        disposition,
        winning_claim_id,
        message,
    })
}

fn apply_resolution(
    class: ConflictClass,
    claims: &[&AliasClaim],
    clause: Option<&ConflictPolicyClause>,
    default_message: &'static str,
) -> (ConflictDisposition, Option<String>, Vec<String>, String) {
    let Some(clause) = clause else {
        return (
            ConflictDisposition::Abstain,
            None,
            Vec::new(),
            default_message.to_string(),
        );
    };

    match &clause.resolution {
        ConflictResolution::Abstain => (
            ConflictDisposition::Abstain,
            None,
            vec![clause.clause_id.clone()],
            format!("policy {} abstains on {:?}", clause.clause_id, class),
        ),
        ConflictResolution::PreferSourceOrder { source_systems } => {
            let winner = claims
                .iter()
                .filter_map(|claim| {
                    source_systems
                        .iter()
                        .position(|source| source == &claim.source_locator.source_system)
                        .map(|index| {
                            (
                                index,
                                recorded_preference_key(claim),
                                claim.claim_id.clone(),
                            )
                        })
                })
                .min()
                .map(|(_, _, claim_id)| claim_id);
            match winner {
                Some(winning_claim_id) => (
                    ConflictDisposition::Resolved,
                    Some(winning_claim_id),
                    vec![clause.clause_id.clone()],
                    format!("policy {} resolved by source precedence", clause.clause_id),
                ),
                None => (
                    ConflictDisposition::Abstain,
                    None,
                    vec![clause.clause_id.clone()],
                    format!(
                        "policy {} could not resolve because no claim source matched the ordered list",
                        clause.clause_id
                    ),
                ),
            }
        }
        ConflictResolution::PreferMostRecentCorrection => {
            let winner = claims
                .iter()
                .max_by_key(|claim| recorded_preference_key(claim))
                .map(|claim| claim.claim_id.clone());
            (
                ConflictDisposition::Resolved,
                winner,
                vec![clause.clause_id.clone()],
                format!(
                    "policy {} resolved by latest recorded correction",
                    clause.clause_id
                ),
            )
        }
        ConflictResolution::AllowHistoricalReuse => (
            ConflictDisposition::HistoricalOnly,
            None,
            vec![clause.clause_id.clone()],
            format!(
                "policy {} keeps the identifier historical by interval",
                clause.clause_id
            ),
        ),
    }
}

fn update_global_visibility(
    record: &ConflictRecord,
    snapshot: &AliasSnapshot,
    blocked_global_claim_ids: &mut BTreeSet<String>,
    winning_global_claim_ids: &mut BTreeSet<String>,
) {
    let claim_map = snapshot
        .active_claims
        .iter()
        .map(|claim| (claim.claim_id.as_str(), claim))
        .collect::<BTreeMap<_, _>>();
    match record.disposition {
        ConflictDisposition::Abstain => {
            for claim_id in &record.claim_ids {
                blocked_global_claim_ids.insert(claim_id.clone());
            }
        }
        ConflictDisposition::Resolved => {
            let Some(winner) = record.winning_claim_id.as_deref() else {
                return;
            };
            for claim_id in &record.claim_ids {
                if claim_id == winner {
                    continue;
                }
                blocked_global_claim_ids.insert(claim_id.clone());
                winning_global_claim_ids.remove(claim_id);
            }
            if let Some(claim) = claim_map.get(winner)
                && matches!(claim.lookup_visibility, LookupVisibility::Global)
            {
                winning_global_claim_ids.insert(winner.to_string());
            }
        }
        ConflictDisposition::HistoricalOnly => {}
    }
}

fn finalize_record(mut record: ConflictRecord) -> TemporalResult<ConflictRecord> {
    record.conflict_id = hash_record(&record)?;
    Ok(record)
}

fn hash_record(record: &ConflictRecord) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ConflictId<'a> {
        version: &'static str,
        class: ConflictClass,
        subject_key: &'a str,
        claim_ids: &'a [String],
        entity_ids: &'a [String],
        policy_clause_ids_used: &'a [String],
        disposition: ConflictDisposition,
        winning_claim_id: &'a Option<String>,
    }

    let bytes = serde_json::to_vec(&ConflictId {
        version: CANON_TEMPORAL_CONFLICT_ARTIFACT_VERSION,
        class: record.class,
        subject_key: &record.subject_key,
        claim_ids: &record.claim_ids,
        entity_ids: &record.entity_ids,
        policy_clause_ids_used: &record.policy_clause_ids_used,
        disposition: record.disposition,
        winning_claim_id: &record.winning_claim_id,
    })
    .map_err(|error| {
        artifact_contract_error(format!("failed to serialize conflict id: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn sorted_claim_ids(claims: &[&AliasClaim]) -> Vec<String> {
    let mut claim_ids = claims
        .iter()
        .map(|claim| claim.claim_id.clone())
        .collect::<Vec<_>>();
    claim_ids.sort();
    claim_ids
}

fn recorded_preference_key(claim: &AliasClaim) -> (Option<&str>, Option<u64>, &str) {
    (
        claim.recorded_time.start_at.as_deref(),
        claim.recorded_time.transaction_seq,
        claim.claim_id.as_str(),
    )
}

fn history_subject_key(class: ConflictClass, claim: &AliasClaim) -> String {
    match class {
        ConflictClass::Retraction => format!("retraction:{}", claim.conflict_key),
        ConflictClass::Correction => format!("correction:{}", claim.conflict_key),
        _ => claim.conflict_key.clone(),
    }
}

fn history_message(class: ConflictClass) -> String {
    match class {
        ConflictClass::Retraction => {
            "later knowledge retracts an earlier alias or anchor claim".to_string()
        }
        ConflictClass::Correction => {
            "later knowledge corrects an earlier alias or anchor claim".to_string()
        }
        _ => "historical event".to_string(),
    }
}

fn active_message(class: ConflictClass) -> &'static str {
    match class {
        ConflictClass::OverlappingExclusiveAnchor => {
            "overlapping exclusive anchors must abstain unless a named policy resolves them"
        }
        ConflictClass::AliasToMultipleEntityClaim => {
            "the same alias cannot point at multiple active entities in one lookup scope"
        }
        ConflictClass::SourceDisagreement => {
            "simultaneous source disagreement must abstain unless policy selects a source"
        }
        _ => "active conflict",
    }
}

fn find_clause(policy: &ConflictPolicy, class: ConflictClass) -> Option<&ConflictPolicyClause> {
    policy
        .clauses
        .iter()
        .find(|clause| clause.conflict_class == class)
}

fn normalize_resolution(resolution: &mut ConflictResolution) -> TemporalResult<()> {
    match resolution {
        ConflictResolution::Abstain
        | ConflictResolution::PreferMostRecentCorrection
        | ConflictResolution::AllowHistoricalReuse => Ok(()),
        ConflictResolution::PreferSourceOrder { source_systems } => {
            let mut normalized = Vec::with_capacity(source_systems.len());
            for source_system in source_systems.drain(..) {
                normalized.push(normalized_non_empty(
                    &source_system,
                    "resolution.source_systems",
                )?);
            }
            if normalized.is_empty() {
                return Err(artifact_contract_error(
                    "prefer_source_order requires at least one source_system",
                ));
            }
            let mut deduped = BTreeSet::new();
            for source_system in &normalized {
                if !deduped.insert(source_system.clone()) {
                    return Err(artifact_contract_error(format!(
                        "duplicate source precedence entry: {source_system}"
                    )));
                }
            }
            *source_systems = normalized;
            Ok(())
        }
    }
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

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::ArtifactContract, message)
}
