#![forbid(unsafe_code)]

//! Declared acceptance authority for new identities. This is an auditable
//! deterministic policy attestation, not a cryptographic human signature.

use super::{
    edge::EdgeEvidenceRecord,
    error::EntityRefusalKind,
    score::ScoreLane,
    solve::{SolveArtifact, SolveReconciliationState},
};
use crate::{Refusal, witness};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionPolicy {
    pub policy_id: String,
    pub require: PromotionRequirements,
    pub audit: PromotionAuditRequirements,
    pub max_auto_accepted_per_run: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionRequirements {
    pub min_adjusted_support_units: u32,
    pub max_hard_cannot_link: u64,
    pub max_soft_anti_merge: u64,
    #[serde(default)]
    pub evidence_all_of: Vec<String>,
    #[serde(default)]
    pub evidence_any_of: Vec<String>,
    pub max_component_surfaces: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionAuditRequirements {
    pub suite: String,
    pub min_pair_precision: serde_json::Number,
    pub min_component_precision: serde_json::Number,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyAcceptance {
    pub strategy_document: String,
    pub strategy_content_hash: String,
    pub policy: PromotionPolicy,
    pub suite_manifest_hash: String,
    pub decisions: Vec<PolicyDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub human_overrides: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub component_id: String,
    pub decision_source: String,
    pub strategy_content_hash: String,
    pub evidence: Vec<EdgeEvidenceRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromotionSection {
    new_ids: String,
    auto_accept: Option<PromotionPolicy>,
}

pub fn load_policy(document: &str) -> Result<Option<PromotionPolicy>, Refusal> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(document).map_err(|_| refusal("invalid_strategy_document"))?;
    let Some(section) = value.get("promotion") else {
        return Ok(None);
    };
    // Existing strategies also use this section for review/write controls.
    // Only the explicit new-ID mode activates this policy contract.
    if section.get("new_ids").is_none() {
        return Ok(None);
    }
    let section: PromotionSection = serde_yaml::from_value(section.clone())
        .map_err(|_| refusal("invalid_promotion_section"))?;
    match section.new_ids.as_str() {
        "escrow_only" => Ok(None),
        "auto_accept" => {
            let policy = section
                .auto_accept
                .ok_or_else(|| refusal("missing_auto_accept_policy"))?;
            let require = &policy.require;
            if policy.policy_id.is_empty()
                || policy.policy_id.chars().any(char::is_whitespace)
                || require.max_hard_cannot_link != 0
                || require.min_adjusted_support_units == 0
                || require.min_adjusted_support_units > 10_000
                || require.max_component_surfaces < 2
                || policy.audit.suite.trim().is_empty()
                || (require.evidence_all_of.is_empty() && require.evidence_any_of.is_empty())
                || require
                    .evidence_all_of
                    .iter()
                    .chain(&require.evidence_any_of)
                    .any(|id| id.trim().is_empty() || id == "*")
            {
                return Err(refusal("invalid_auto_accept_requirements"));
            }
            precision_basis_points(&policy.audit.min_pair_precision)?;
            precision_basis_points(&policy.audit.min_component_precision)?;
            Ok(Some(policy))
        }
        _ => Err(refusal("invalid_new_id_policy")),
    }
}

pub fn precision_basis_points(number: &serde_json::Number) -> Result<u64, Refusal> {
    let text = number.to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    if !whole
        .bytes()
        .chain(fraction.bytes())
        .all(|b| b.is_ascii_digit())
        || fraction.len() > 4
    {
        return Err(refusal("precision_requires_at_most_four_decimal_places"));
    }
    let units = whole
        .parse::<u64>()
        .ok()
        .and_then(|whole| whole.checked_mul(10_000))
        .and_then(|whole| {
            fraction
                .parse::<u64>()
                .unwrap_or(0)
                .checked_mul(10u64.pow(4 - fraction.len() as u32))
                .and_then(|part| whole.checked_add(part))
        })
        .ok_or_else(|| refusal("invalid_precision_floor"))?;
    if units == 0 || units > 10_000 {
        return Err(refusal("invalid_precision_floor"));
    }
    Ok(units)
}

fn operator_matches(pattern: &str, operator: &str) -> bool {
    // Prefix matching is explicit (trailing '*'); an exact ID never happens
    // to authorize a different operator sharing its spelling.
    pattern
        .strip_suffix('*')
        .map_or(operator == pattern, |prefix| operator.starts_with(prefix))
}

fn qualifying_edge(edge: &EdgeEvidenceRecord, policy: &PromotionPolicy) -> bool {
    let req = &policy.require;
    if edge.has_hard_cannot_link
        || edge.hits.iter().any(|hit| hit.hard_cannot_link)
        || edge.pair_score_total.as_u32() < req.min_adjusted_support_units
    {
        return false;
    }
    let has = |pattern: &String| {
        edge.hits.iter().any(|hit| {
            hit.lane == ScoreLane::Support
                && hit.score_units.as_u32() > 0
                && operator_matches(pattern, &hit.operator_id)
        })
    };
    req.evidence_all_of.iter().all(has)
        && (req.evidence_any_of.is_empty() || req.evidence_any_of.iter().any(has))
}

pub fn apply_policy(
    artifact: &mut SolveArtifact,
    document: String,
    edges: &[EdgeEvidenceRecord],
    human_overrides: &[serde_json::Value],
) -> Result<Option<PolicyAcceptance>, Refusal> {
    let Some(policy) = load_policy(&document)? else {
        return Ok(None);
    };
    let strategy_content_hash = witness::hash_bytes(document.as_bytes());
    let mut edges_by_component: BTreeMap<&str, Vec<&EdgeEvidenceRecord>> = BTreeMap::new();
    let owners: BTreeMap<_, _> = artifact
        .entities
        .iter()
        .flat_map(|entity| {
            entity
                .surface_ids
                .iter()
                .map(|surface| (surface.as_str(), entity.component_id.as_str()))
        })
        .collect();
    for edge in edges {
        if let Some(owner) = owners.get(edge.left_surface_id.as_str())
            && owners.get(edge.right_surface_id.as_str()) == Some(owner)
        {
            edges_by_component.entry(owner).or_default().push(edge);
        }
    }
    let mut decisions = Vec::new();
    for entity in &artifact.entities {
        if human_overrides.iter().any(|record| {
            record["left"]
                .as_str()
                .is_some_and(|left| entity.surface_ids.iter().any(|id| id == left))
                && record["right"]
                    .as_str()
                    .is_some_and(|right| entity.surface_ids.iter().any(|id| id == right))
        }) {
            continue;
        }
        if entity.state != SolveReconciliationState::Escrow
            || !entity.incumbent_canonical_ids.is_empty()
            || entity.hard_cannot_link_count != 0
            || entity.soft_anti_merge_warning_count > policy.require.max_soft_anti_merge
            || entity.adjusted_support_score_units.as_u32()
                < policy.require.min_adjusted_support_units
            || entity.surface_ids.len() < 2
            || entity.surface_ids.len() > policy.require.max_component_surfaces
        {
            continue;
        }
        let component_edges = edges_by_component
            .get(entity.component_id.as_str())
            .cloned()
            .unwrap_or_default();
        if component_edges.iter().any(|edge| {
            edge.has_hard_cannot_link || edge.hits.iter().any(|hit| hit.hard_cannot_link)
        }) {
            continue;
        }
        let evidence: Vec<_> = component_edges
            .into_iter()
            .filter(|edge| qualifying_edge(edge, &policy))
            .cloned()
            .collect();
        // Every member must connect through policy-qualified edges. A strong
        // sub-pair cannot lend authority to the rest of a weak component.
        let mut reached = BTreeSet::from([entity.surface_ids[0].as_str()]);
        loop {
            let previous = reached.len();
            for edge in &evidence {
                if reached.contains(edge.left_surface_id.as_str())
                    || reached.contains(edge.right_surface_id.as_str())
                {
                    reached.insert(&edge.left_surface_id);
                    reached.insert(&edge.right_surface_id);
                }
            }
            if reached.len() == previous {
                break;
            }
        }
        if reached.len() != entity.surface_ids.len() {
            continue;
        }
        decisions.push(PolicyDecision {
            component_id: entity.component_id.clone(),
            decision_source: format!("policy:{}", policy.policy_id),
            strategy_content_hash: strategy_content_hash.clone(),
            evidence,
        });
    }
    if policy
        .max_auto_accepted_per_run
        .is_some_and(|limit| decisions.len() > limit)
    {
        return Err(refusal("auto_accept_run_ceiling_exceeded"));
    }
    let accepted: BTreeSet<_> = decisions
        .iter()
        .map(|decision| decision.component_id.as_str())
        .collect();
    for entity in &mut artifact.entities {
        if accepted.contains(entity.component_id.as_str()) {
            entity.state = SolveReconciliationState::PromotableNew;
            entity.reason = format!("auto_accept:{}", policy.policy_id);
            entity.canonical_id = Some(format!(
                "entity:{}",
                entity.component_id.trim_start_matches("component:")
            ));
            entity.candidate_id = entity.canonical_id.clone();
        }
    }
    let manifest =
        std::fs::read(std::path::Path::new(&policy.audit.suite).join("policy_suite.json"))
            .map_err(|_| refusal("missing_frozen_policy_suite"))?;
    Ok(Some(PolicyAcceptance {
        strategy_document: document,
        strategy_content_hash,
        policy,
        suite_manifest_hash: witness::hash_bytes(&manifest),
        decisions,
        human_overrides: human_overrides.to_vec(),
    }))
}

pub(crate) fn refusal(reason: &str) -> Refusal {
    EntityRefusalKind::AuditGate.to_refusal("Policy acceptance requires declared, verified authority",
        json!({"reason": reason, "writes_performed": false}),
        Some("Run canon entity audit on the solve artifact with the declared frozen suite, or use human review".into()))
}

pub(crate) fn read_human_overrides(
    registry: &std::path::Path,
) -> Result<Vec<serde_json::Value>, Refusal> {
    let bytes = match std::fs::read_to_string(registry.join("_escrow/cannot_link.jsonl")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(refusal("unreadable_human_overrides")),
    };
    bytes
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let record: serde_json::Value =
                serde_json::from_str(line).map_err(|_| refusal("invalid_human_override"))?;
            if record["left"].as_str().is_none() || record["right"].as_str().is_none() {
                return Err(refusal("invalid_human_override_pair"));
            }
            Ok(record)
        })
        .collect()
}

/// Frozen inputs and independently supplied entity labels. Pins are mandatory:
/// changing a corpus or profile requires a new suite manifest and a new run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenPolicySuite {
    rows: String,
    rows_hash: String,
    profile: String,
    profile_hash: String,
    registry: String,
    registry_snapshot_hash: String,
    gold: String,
    gold_hash: String,
    gold_provenance: String,
    work_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldIdentity {
    surface_id: String,
    entity_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyAuditResult {
    pub policy_id: String,
    pub strategy_content_hash: String,
    pub suite_manifest_hash: String,
    pub evaluated_solve_hash: String,
    pub profile_content_hash: String,
    pub gold_hash: String,
    pub gold_provenance: String,
    pub accepted_pairs: u64,
    pub correct_pairs: u64,
    pub accepted_components: u64,
    pub correct_components: u64,
    pub labeled_surfaces: u64,
    pub labeled_entities: u64,
}

fn pinned_file(base: &std::path::Path, path: &str, expected: &str) -> Result<Vec<u8>, Refusal> {
    let bytes = std::fs::read(base.join(path)).map_err(|_| refusal("unreadable_suite_input"))?;
    if witness::hash_bytes(&bytes) != expected {
        return Err(refusal("suite_input_hash_mismatch"));
    }
    Ok(bytes)
}

/// Run the same declared strategy on the pinned corpus, then score all pairs
/// and whole components accepted by the policy. No caller-supplied `passed`
/// boolean or precision claim is accepted as evidence.
pub fn audit_policy(
    source: &SolveArtifact,
    suite_dir: &std::path::Path,
) -> Result<Option<PolicyAuditResult>, Refusal> {
    let Some(acceptance) = &source.policy_acceptance else {
        return Ok(None);
    };
    validate_attestation(source)?;
    let declared = std::fs::canonicalize(&acceptance.policy.audit.suite)
        .map_err(|_| refusal("unreadable_declared_suite"))?;
    if std::fs::canonicalize(suite_dir).map_err(|_| refusal("unreadable_audit_suite"))? != declared
    {
        return Err(refusal("wrong_policy_audit_suite"));
    }
    let manifest_bytes = pinned_file(
        &declared,
        "policy_suite.json",
        &acceptance.suite_manifest_hash,
    )?;
    let manifest: FrozenPolicySuite =
        serde_json::from_slice(&manifest_bytes).map_err(|_| refusal("invalid_policy_suite"))?;
    pinned_file(&declared, &manifest.rows, &manifest.rows_hash)?;
    pinned_file(&declared, &manifest.profile, &manifest.profile_hash)?;
    let gold_bytes = pinned_file(&declared, &manifest.gold, &manifest.gold_hash)?;
    if manifest.gold_provenance.trim().is_empty() {
        return Err(refusal("missing_gold_provenance"));
    }
    let labels: Vec<GoldIdentity> =
        serde_json::from_slice(&gold_bytes).map_err(|_| refusal("invalid_gold_labels"))?;
    let mut gold = BTreeMap::new();
    for label in labels {
        if label.entity_id.trim().is_empty()
            || gold.insert(label.surface_id, label.entity_id).is_some()
        {
            return Err(refusal("duplicate_or_empty_gold_label"));
        }
    }
    let label_entities: BTreeSet<_> = gold.values().collect();
    if gold.len() < 3 || label_entities.len() < 2 || label_entities.len() == gold.len() {
        return Err(refusal(
            "suite_requires_positive_and_negative_identity_labels",
        ));
    }
    let work = declared
        .join(&manifest.work_dir)
        .join(acceptance.strategy_content_hash.replace(':', "_"))
        .join(acceptance.suite_manifest_hash.replace(':', "_"));
    std::fs::create_dir_all(&work).map_err(|_| refusal("audit_work_dir_unwritable"))?;
    let strategy = work.join("strategy.yaml");
    match std::fs::read(&strategy) {
        Ok(bytes) if bytes != acceptance.strategy_document.as_bytes() => {
            return Err(refusal("audit_strategy_path_collision"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::write(&strategy, &acceptance.strategy_document)
                .map_err(|_| refusal("audit_strategy_unwritable"))?;
        }
        Err(_) => return Err(refusal("audit_strategy_unreadable")),
    }
    let rows = declared.join(&manifest.rows);
    let profile = declared.join(&manifest.profile);
    let registry = declared.join(&manifest.registry);
    super::run::run_entity_workbench(super::run::EntityRunRequest {
        rows: &rows,
        profile: profile
            .to_str()
            .ok_or_else(|| refusal("invalid_profile_path"))?,
        strategy: &strategy,
        registry: &registry,
        work_dir: &work,
    })?;
    let solve_bytes =
        super::run::read_entity_run_committed_publication_logical_bytes(&work, "solve/solve.json")?
            .ok_or_else(|| refusal("missing_audit_solve"))?;
    let solve: SolveArtifact =
        serde_json::from_slice(&solve_bytes).map_err(|_| refusal("invalid_audit_solve"))?;
    let profile_hash = solve
        .metadata
        .profile
        .content_hash
        .clone()
        .ok_or_else(|| refusal("missing_profile_hash"))?;
    if source.metadata.profile.content_hash.as_ref() != Some(&profile_hash)
        || profile_hash != manifest.profile_hash
        || solve
            .metadata
            .input
            .as_ref()
            .map(|input| input.content_hash.as_str())
            != Some(manifest.rows_hash.as_str())
        || solve.metadata.registry_snapshot.lookup_snapshot_hash != manifest.registry_snapshot_hash
    {
        return Err(refusal("audit_profile_or_registry_mismatch"));
    }
    let surfaces_bytes = std::fs::read(work.join("prepare/surfaces.jsonl"))
        .map_err(|_| refusal("missing_audit_surfaces"))?;
    let surfaces: BTreeSet<String> = std::str::from_utf8(&surfaces_bytes)
        .map_err(|_| refusal("invalid_audit_surfaces"))?
        .lines()
        .map(|line| {
            serde_json::from_str::<super::prepare::PreparedSurfaceRecord>(line)
                .map(|surface| surface.surface_id)
                .map_err(|_| refusal("invalid_audit_surface"))
        })
        .collect::<Result<_, _>>()?;
    if surfaces != gold.keys().cloned().collect() {
        return Err(refusal("gold_must_cover_exact_audit_surface_universe"));
    }
    let mut result = PolicyAuditResult {
        policy_id: acceptance.policy.policy_id.clone(),
        strategy_content_hash: acceptance.strategy_content_hash.clone(),
        suite_manifest_hash: acceptance.suite_manifest_hash.clone(),
        evaluated_solve_hash: solve.artifact_content_hash.clone(),
        profile_content_hash: profile_hash,
        gold_hash: manifest.gold_hash,
        gold_provenance: manifest.gold_provenance,
        accepted_pairs: 0,
        correct_pairs: 0,
        accepted_components: 0,
        correct_components: 0,
        labeled_surfaces: gold.len() as u64,
        labeled_entities: label_entities.len() as u64,
    };
    for entity in solve
        .entities
        .iter()
        .filter(|entity| entity.state == SolveReconciliationState::PromotableNew)
    {
        result.accepted_components += 1;
        let labels: Vec<_> = entity
            .surface_ids
            .iter()
            .map(|surface| &gold[surface])
            .collect();
        if labels.iter().all(|label| label == &labels[0]) {
            result.correct_components += 1;
        }
        for (index, left) in labels.iter().enumerate() {
            for right in &labels[index + 1..] {
                result.accepted_pairs += 1;
                result.correct_pairs += u64::from(left == right);
            }
        }
    }
    validate_audit_result(acceptance, &result)?;
    Ok(Some(result))
}

pub fn validate_audit_result(
    acceptance: &PolicyAcceptance,
    audit: &PolicyAuditResult,
) -> Result<(), Refusal> {
    if audit.policy_id != acceptance.policy.policy_id
        || audit.strategy_content_hash != acceptance.strategy_content_hash
        || audit.suite_manifest_hash != acceptance.suite_manifest_hash
    {
        return Err(refusal("policy_audit_binding_mismatch"));
    }
    for (correct, count, floor) in [
        (
            audit.correct_pairs,
            audit.accepted_pairs,
            &acceptance.policy.audit.min_pair_precision,
        ),
        (
            audit.correct_components,
            audit.accepted_components,
            &acceptance.policy.audit.min_component_precision,
        ),
    ] {
        if count == 0
            || correct > count
            || u128::from(correct) * 10_000
                < u128::from(count) * u128::from(precision_basis_points(floor)?)
        {
            return Err(refusal("policy_precision_floor_failed"));
        }
    }
    Ok(())
}

pub fn validate_attestation(source: &SolveArtifact) -> Result<(), Refusal> {
    let acceptance = source
        .policy_acceptance
        .as_ref()
        .ok_or_else(|| refusal("missing_policy_attestation"))?;
    if witness::hash_bytes(acceptance.strategy_document.as_bytes())
        != acceptance.strategy_content_hash
        || load_policy(&acceptance.strategy_document)?.as_ref() != Some(&acceptance.policy)
        || witness::hash_bytes(format!("{}:solve", acceptance.strategy_content_hash).as_bytes())
            != source.metadata.strategy.content_hash
    {
        return Err(refusal("policy_strategy_binding_mismatch"));
    }
    let mut replay = source.clone();
    for entity in &mut replay.entities {
        if entity.state == SolveReconciliationState::PromotableNew {
            entity.state = SolveReconciliationState::Escrow;
            entity.canonical_id = None;
            entity.candidate_id = None;
        }
    }
    let edges: Vec<_> = acceptance
        .decisions
        .iter()
        .flat_map(|decision| decision.evidence.clone())
        .collect();
    let expected = apply_policy(
        &mut replay,
        acceptance.strategy_document.clone(),
        &edges,
        &acceptance.human_overrides,
    )?
    .ok_or_else(|| refusal("missing_policy"))?;
    if expected != *acceptance || replay.entities != source.entities {
        return Err(refusal("policy_decision_attestation_mismatch"));
    }
    Ok(())
}
