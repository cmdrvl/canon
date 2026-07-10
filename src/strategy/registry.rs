#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{error::Error, fmt};

pub fn strategy_registry_contract_version() -> &'static str {
    "canon.strategy.registry.v1"
}

pub fn strategy_audit_receipt_version() -> &'static str {
    "canon.strategy.audit_receipt.v1"
}

pub fn strategy_promotion_receipt_version() -> &'static str {
    "canon.strategy.promotion.v1"
}

pub fn strategy_selection_receipt_version() -> &'static str {
    "canon.strategy.selection.v1"
}

pub fn strategy_project_lock_version() -> &'static str {
    "canon.strategy.project_lock.v1"
}

pub fn strategy_use_receipt_version() -> &'static str {
    "canon.strategy.use.v1"
}

pub type StrategyLifecycleResult<T> = Result<T, StrategyLifecycleError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyLifecycleErrorCode {
    SerializationFailure,
    AuditRejected,
    StaleAudit,
    ThresholdMismatch,
    RunnerPolicyMismatch,
    FixtureCorpusMismatch,
    SelectionKeyMismatch,
    PackageDigestMismatch,
    RegistryRace,
    RegistryAncestryBroken,
    VersionBumpRequired,
    PackageDeprecated,
    NoActiveChampion,
    DuplicateActiveChampion,
    PromotionChainBroken,
    ProjectLockStale,
    SelectionProofStale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyLifecycleError {
    pub code: StrategyLifecycleErrorCode,
    pub message: String,
    pub detail: Value,
}

impl StrategyLifecycleError {
    pub fn new(
        code: StrategyLifecycleErrorCode,
        message: impl Into<String>,
        detail: Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail,
        }
    }
}

impl fmt::Display for StrategyLifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for StrategyLifecycleError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StrategySelectionKey {
    IdentityEvidence {
        profile_id: String,
        skill_hash: String,
    },
    RecordLinkage {
        linkage_map_id: String,
        skill_hash: String,
    },
    SchemaTransform {
        schema_fingerprint: String,
        skill_hash: String,
    },
    TaskTransform {
        task: String,
        skill_hash: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyCompatibilityLevel {
    Exact,
    Compatible,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyChampionStatus {
    Active,
    Superseded,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPackageStatus {
    Available,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyAuditDecision {
    Proceed,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditReceipt {
    pub version: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub fixture_corpus_digest: String,
    pub thresholds_digest: String,
    pub runner_policy_digest: String,
    pub registry_parent_digest: String,
    pub deterministic_output_hash: String,
    pub passed: bool,
    pub decision: StrategyAuditDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPromotionReceipt {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub compatibility: StrategyCompatibilityLevel,
    pub fixture_corpus_digest: String,
    pub thresholds_digest: String,
    pub runner_policy_digest: String,
    pub registry_parent_digest: String,
    pub audit_receipt_digest: String,
    pub audit_output_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyRegistryEntry {
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub compatibility: StrategyCompatibilityLevel,
    pub champion_status: StrategyChampionStatus,
    pub package_status: StrategyPackageStatus,
    pub promotion: StrategyPromotionReceipt,
    pub audit: StrategyAuditReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyRegistrySnapshot {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub ancestry: Vec<String>,
    pub entries: Vec<StrategyRegistryEntry>,
}

impl StrategyRegistrySnapshot {
    pub fn empty(registry_id: impl Into<String>, registry_version: impl Into<String>) -> Self {
        Self {
            version: strategy_registry_contract_version().to_string(),
            registry_id: registry_id.into(),
            registry_version: registry_version.into(),
            ancestry: Vec::new(),
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySelectionAlternative {
    pub package_digest: String,
    pub compatibility: StrategyCompatibilityLevel,
    pub champion_status: StrategyChampionStatus,
    pub package_status: StrategyPackageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySelectionProof {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_digest: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub compatibility: StrategyCompatibilityLevel,
    pub promotion_receipt_digest: String,
    pub audit_receipt_digest: String,
    pub fixture_corpus_digest: String,
    pub thresholds_digest: String,
    pub runner_policy_digest: String,
    pub alternatives: Vec<StrategySelectionAlternative>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProjectLock {
    pub version: String,
    pub project_id: String,
    pub project_digest: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_digest: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub selection_proof_digest: String,
    pub promotion_receipt_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyUseReceipt {
    pub version: String,
    pub project_id: String,
    pub project_digest: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_digest: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub selection_proof_digest: String,
    pub promotion_receipt_digest: String,
    pub audit_receipt_digest: String,
}

pub fn canonical_digest<T: Serialize>(value: &T) -> StrategyLifecycleResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::SerializationFailure,
            "failed to serialize lifecycle receipt",
            json!({ "error": error.to_string() }),
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn registry_snapshot_digest(
    snapshot: &StrategyRegistrySnapshot,
) -> StrategyLifecycleResult<String> {
    canonical_digest(snapshot)
}

pub fn select_champion(
    snapshot: &StrategyRegistrySnapshot,
    selection_key: &StrategySelectionKey,
) -> StrategyLifecycleResult<StrategySelectionProof> {
    let registry_digest = registry_snapshot_digest(snapshot)?;
    let mut matching = snapshot
        .entries
        .iter()
        .filter(|entry| &entry.selection_key == selection_key)
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| left.package_digest.cmp(&right.package_digest));

    let active = matching
        .iter()
        .copied()
        .filter(|entry| entry.champion_status == StrategyChampionStatus::Active)
        .collect::<Vec<_>>();

    let champion = match active.as_slice() {
        [] => {
            return Err(StrategyLifecycleError::new(
                StrategyLifecycleErrorCode::NoActiveChampion,
                "no active champion exists for the requested typed strategy key",
                json!({
                    "registry_id": snapshot.registry_id,
                    "registry_version": snapshot.registry_version,
                    "selection_key": selection_key,
                }),
            ));
        }
        [entry] => *entry,
        entries => {
            return Err(StrategyLifecycleError::new(
                StrategyLifecycleErrorCode::DuplicateActiveChampion,
                "multiple active champions exist for one typed strategy key",
                json!({
                    "selection_key": selection_key,
                    "active_packages": entries
                        .iter()
                        .map(|entry| entry.package_digest.clone())
                        .collect::<Vec<_>>(),
                }),
            ));
        }
    };

    verify_entry_chain(snapshot, champion)?;
    if champion.package_status == StrategyPackageStatus::Deprecated {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PackageDeprecated,
            "the selected champion package is deprecated and cannot be used",
            json!({
                "selection_key": selection_key,
                "package_digest": champion.package_digest,
            }),
        ));
    }

    let alternatives = matching
        .into_iter()
        .filter(|entry| !std::ptr::eq(*entry, champion))
        .map(|entry| StrategySelectionAlternative {
            package_digest: entry.package_digest.clone(),
            compatibility: entry.compatibility,
            champion_status: entry.champion_status,
            package_status: entry.package_status,
        })
        .collect::<Vec<_>>();

    Ok(StrategySelectionProof {
        version: strategy_selection_receipt_version().to_string(),
        registry_id: snapshot.registry_id.clone(),
        registry_version: snapshot.registry_version.clone(),
        registry_digest,
        selection_key: champion.selection_key.clone(),
        package_digest: champion.package_digest.clone(),
        compatibility: champion.compatibility,
        promotion_receipt_digest: canonical_digest(&champion.promotion)?,
        audit_receipt_digest: canonical_digest(&champion.audit)?,
        fixture_corpus_digest: champion.promotion.fixture_corpus_digest.clone(),
        thresholds_digest: champion.promotion.thresholds_digest.clone(),
        runner_policy_digest: champion.promotion.runner_policy_digest.clone(),
        alternatives,
    })
}

pub fn create_project_lock(
    project_id: impl Into<String>,
    project_digest: impl Into<String>,
    selection: &StrategySelectionProof,
) -> StrategyLifecycleResult<StrategyProjectLock> {
    Ok(StrategyProjectLock {
        version: strategy_project_lock_version().to_string(),
        project_id: project_id.into(),
        project_digest: project_digest.into(),
        registry_id: selection.registry_id.clone(),
        registry_version: selection.registry_version.clone(),
        registry_digest: selection.registry_digest.clone(),
        selection_key: selection.selection_key.clone(),
        package_digest: selection.package_digest.clone(),
        selection_proof_digest: canonical_digest(selection)?,
        promotion_receipt_digest: selection.promotion_receipt_digest.clone(),
    })
}

pub fn use_project_lock(
    snapshot: &StrategyRegistrySnapshot,
    project_lock: &StrategyProjectLock,
) -> StrategyLifecycleResult<StrategyUseReceipt> {
    let registry_digest = registry_snapshot_digest(snapshot)?;
    if registry_digest != project_lock.registry_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::ProjectLockStale,
            "project lock was taken against a different registry digest",
            json!({
                "project_id": project_lock.project_id,
                "expected_registry_digest": project_lock.registry_digest,
                "actual_registry_digest": registry_digest,
            }),
        ));
    }

    let selection = select_champion(snapshot, &project_lock.selection_key)?;
    if selection.package_digest != project_lock.package_digest
        || selection.promotion_receipt_digest != project_lock.promotion_receipt_digest
    {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::ProjectLockStale,
            "project lock no longer points at the current active champion",
            json!({
                "project_id": project_lock.project_id,
                "locked_package_digest": project_lock.package_digest,
                "champion_package_digest": selection.package_digest,
            }),
        ));
    }

    let selection_proof_digest = canonical_digest(&selection)?;
    if selection_proof_digest != project_lock.selection_proof_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::SelectionProofStale,
            "project lock selection proof digest does not match the current verified proof",
            json!({
                "project_id": project_lock.project_id,
                "expected_selection_proof_digest": project_lock.selection_proof_digest,
                "actual_selection_proof_digest": selection_proof_digest,
            }),
        ));
    }

    Ok(StrategyUseReceipt {
        version: strategy_use_receipt_version().to_string(),
        project_id: project_lock.project_id.clone(),
        project_digest: project_lock.project_digest.clone(),
        registry_id: selection.registry_id,
        registry_version: selection.registry_version,
        registry_digest: selection.registry_digest,
        selection_key: selection.selection_key,
        package_digest: selection.package_digest,
        selection_proof_digest,
        promotion_receipt_digest: selection.promotion_receipt_digest,
        audit_receipt_digest: selection.audit_receipt_digest,
    })
}

pub fn verify_entry_chain(
    snapshot: &StrategyRegistrySnapshot,
    entry: &StrategyRegistryEntry,
) -> StrategyLifecycleResult<()> {
    if entry.audit.version != strategy_audit_receipt_version() {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PromotionChainBroken,
            "strategy audit receipt version is not the canonical lifecycle version",
            json!({
                "expected": strategy_audit_receipt_version(),
                "actual": entry.audit.version,
            }),
        ));
    }

    if entry.promotion.version != strategy_promotion_receipt_version() {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PromotionChainBroken,
            "strategy promotion receipt version is not the canonical lifecycle version",
            json!({
                "expected": strategy_promotion_receipt_version(),
                "actual": entry.promotion.version,
            }),
        ));
    }

    if !entry.audit.passed || entry.audit.decision != StrategyAuditDecision::Proceed {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::AuditRejected,
            "the selected champion does not carry a passing audit receipt",
            json!({
                "selection_key": entry.selection_key,
                "package_digest": entry.package_digest,
                "decision": entry.audit.decision,
                "passed": entry.audit.passed,
            }),
        ));
    }

    if entry.audit.selection_key != entry.selection_key
        || entry.promotion.selection_key != entry.selection_key
    {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::SelectionKeyMismatch,
            "audit or promotion receipt selection key does not match the registry entry",
            json!({
                "entry_key": entry.selection_key,
                "audit_key": entry.audit.selection_key,
                "promotion_key": entry.promotion.selection_key,
            }),
        ));
    }

    if entry.audit.package_digest != entry.package_digest
        || entry.promotion.package_digest != entry.package_digest
    {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PackageDigestMismatch,
            "audit or promotion receipt package digest does not match the registry entry",
            json!({
                "entry_package_digest": entry.package_digest,
                "audit_package_digest": entry.audit.package_digest,
                "promotion_package_digest": entry.promotion.package_digest,
            }),
        ));
    }

    if entry.audit.fixture_corpus_digest != entry.promotion.fixture_corpus_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::FixtureCorpusMismatch,
            "promotion receipt fixture corpus digest differs from the audit receipt",
            json!({
                "selection_key": entry.selection_key,
                "audit_fixture_corpus_digest": entry.audit.fixture_corpus_digest,
                "promotion_fixture_corpus_digest": entry.promotion.fixture_corpus_digest,
            }),
        ));
    }

    if entry.audit.thresholds_digest != entry.promotion.thresholds_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::ThresholdMismatch,
            "promotion receipt thresholds digest differs from the audit receipt",
            json!({
                "selection_key": entry.selection_key,
                "audit_thresholds_digest": entry.audit.thresholds_digest,
                "promotion_thresholds_digest": entry.promotion.thresholds_digest,
            }),
        ));
    }

    if entry.audit.runner_policy_digest != entry.promotion.runner_policy_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::RunnerPolicyMismatch,
            "promotion receipt runner policy digest differs from the audit receipt",
            json!({
                "selection_key": entry.selection_key,
                "audit_runner_policy_digest": entry.audit.runner_policy_digest,
                "promotion_runner_policy_digest": entry.promotion.runner_policy_digest,
            }),
        ));
    }

    if entry.audit.registry_parent_digest != entry.promotion.registry_parent_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::StaleAudit,
            "promotion receipt registry parent digest differs from the audit receipt",
            json!({
                "selection_key": entry.selection_key,
                "audit_registry_parent_digest": entry.audit.registry_parent_digest,
                "promotion_registry_parent_digest": entry.promotion.registry_parent_digest,
            }),
        ));
    }

    if entry.promotion.registry_id != snapshot.registry_id
        || !snapshot
            .ancestry
            .iter()
            .any(|digest| digest == &entry.promotion.registry_parent_digest)
    {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::RegistryAncestryBroken,
            "promotion receipt does not point at a known registry ancestor digest",
            json!({
                "registry_id": snapshot.registry_id,
                "known_ancestry": snapshot.ancestry,
                "promotion_registry_parent_digest": entry.promotion.registry_parent_digest,
            }),
        ));
    }

    let audit_receipt_digest = canonical_digest(&entry.audit)?;
    if audit_receipt_digest != entry.promotion.audit_receipt_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PromotionChainBroken,
            "promotion receipt does not match the sealed audit receipt digest",
            json!({
                "expected_audit_receipt_digest": entry.promotion.audit_receipt_digest,
                "actual_audit_receipt_digest": audit_receipt_digest,
            }),
        ));
    }

    Ok(())
}
