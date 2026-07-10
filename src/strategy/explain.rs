#![forbid(unsafe_code)]

use super::registry::{
    StrategyChampionStatus, StrategyLifecycleResult, StrategyPackageStatus, StrategyProjectLock,
    StrategySelectionKey, StrategySelectionProof, registry_snapshot_digest, select_champion,
};
use serde::{Deserialize, Serialize};

pub fn strategy_explain_contract_version() -> &'static str {
    "canon.strategy.explain.v1"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyProjectImpactDisposition {
    CurrentChampionPinned,
    StaleLockSameChampion,
    StaleLockWouldChangeChampion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExplainAlternative {
    pub package_digest: String,
    pub compatibility: super::registry::StrategyCompatibilityLevel,
    pub champion_status: StrategyChampionStatus,
    pub package_status: StrategyPackageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExplainEvidence {
    pub promotion_receipt_digest: String,
    pub audit_receipt_digest: String,
    pub fixture_corpus_digest: String,
    pub thresholds_digest: String,
    pub runner_policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExplainChampion {
    pub package_digest: String,
    pub compatibility: super::registry::StrategyCompatibilityLevel,
    pub evidence: StrategyExplainEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProjectImpact {
    pub project_id: String,
    pub locked_package_digest: String,
    pub champion_package_digest: String,
    pub disposition: StrategyProjectImpactDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExplainOutput {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_digest: String,
    pub selection_key: StrategySelectionKey,
    pub champion: StrategyExplainChampion,
    pub alternatives: Vec<StrategyExplainAlternative>,
    pub project_impact: Vec<StrategyProjectImpact>,
}

pub fn explain(
    snapshot: &super::registry::StrategyRegistrySnapshot,
    selection_key: &StrategySelectionKey,
    project_locks: &[StrategyProjectLock],
) -> StrategyLifecycleResult<StrategyExplainOutput> {
    let selection = select_champion(snapshot, selection_key)?;
    let registry_digest = registry_snapshot_digest(snapshot)?;
    let champion = StrategyExplainChampion {
        package_digest: selection.package_digest.clone(),
        compatibility: selection.compatibility,
        evidence: StrategyExplainEvidence {
            promotion_receipt_digest: selection.promotion_receipt_digest.clone(),
            audit_receipt_digest: selection.audit_receipt_digest.clone(),
            fixture_corpus_digest: selection.fixture_corpus_digest.clone(),
            thresholds_digest: selection.thresholds_digest.clone(),
            runner_policy_digest: selection.runner_policy_digest.clone(),
        },
    };

    let mut project_impact =
        project_impact(selection_key, &selection, project_locks, &registry_digest);
    project_impact.sort_by(|left, right| left.project_id.cmp(&right.project_id));

    let mut alternatives = selection
        .alternatives
        .into_iter()
        .map(|alternative| StrategyExplainAlternative {
            package_digest: alternative.package_digest,
            compatibility: alternative.compatibility,
            champion_status: alternative.champion_status,
            package_status: alternative.package_status,
        })
        .collect::<Vec<_>>();
    alternatives.sort_by(|left, right| left.package_digest.cmp(&right.package_digest));

    Ok(StrategyExplainOutput {
        version: strategy_explain_contract_version().to_string(),
        registry_id: selection.registry_id,
        registry_version: selection.registry_version,
        registry_digest,
        selection_key: selection.selection_key,
        champion,
        alternatives,
        project_impact,
    })
}

fn project_impact(
    selection_key: &StrategySelectionKey,
    selection: &StrategySelectionProof,
    project_locks: &[StrategyProjectLock],
    registry_digest: &str,
) -> Vec<StrategyProjectImpact> {
    project_locks
        .iter()
        .filter(|lock| &lock.selection_key == selection_key)
        .map(|lock| {
            let disposition = if lock.registry_digest == *registry_digest
                && lock.package_digest == selection.package_digest
            {
                StrategyProjectImpactDisposition::CurrentChampionPinned
            } else if lock.package_digest == selection.package_digest {
                StrategyProjectImpactDisposition::StaleLockSameChampion
            } else {
                StrategyProjectImpactDisposition::StaleLockWouldChangeChampion
            };

            StrategyProjectImpact {
                project_id: lock.project_id.clone(),
                locked_package_digest: lock.package_digest.clone(),
                champion_package_digest: selection.package_digest.clone(),
                disposition,
            }
        })
        .collect()
}
