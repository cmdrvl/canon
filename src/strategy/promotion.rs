#![forbid(unsafe_code)]

use super::registry::{
    StrategyAuditDecision, StrategyAuditReceipt, StrategyChampionStatus,
    StrategyCompatibilityLevel, StrategyLifecycleError, StrategyLifecycleErrorCode,
    StrategyLifecycleResult, StrategyPackageStatus, StrategyPromotionReceipt,
    StrategyRegistryEntry, StrategyRegistrySnapshot, StrategySelectionKey, canonical_digest,
    registry_snapshot_digest, strategy_promotion_receipt_version,
    strategy_registry_contract_version,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPromotionRequest {
    pub registry_version: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub compatibility: StrategyCompatibilityLevel,
    pub fixture_corpus_digest: String,
    pub thresholds_digest: String,
    pub runner_policy_digest: String,
    pub package_status: StrategyPackageStatus,
    pub audit: StrategyAuditReceipt,
    pub expected_registry_parent_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPromotionResult {
    pub registry: StrategyRegistrySnapshot,
    pub entry: StrategyRegistryEntry,
    pub receipt: StrategyPromotionReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDeprecationRequest {
    pub registry_version: String,
    pub selection_key: StrategySelectionKey,
    pub package_digest: String,
    pub expected_registry_parent_digest: String,
}

pub fn promote(
    snapshot: &StrategyRegistrySnapshot,
    request: StrategyPromotionRequest,
) -> StrategyLifecycleResult<StrategyPromotionResult> {
    if request.registry_version == snapshot.registry_version {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::VersionBumpRequired,
            "promotion requires an explicit new registry version",
            json!({
                "current_registry_version": snapshot.registry_version,
                "next_registry_version": request.registry_version,
            }),
        ));
    }

    let current_registry_digest = registry_snapshot_digest(snapshot)?;
    if request.expected_registry_parent_digest != current_registry_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::RegistryRace,
            "promotion request was prepared against a different registry digest",
            json!({
                "expected_registry_parent_digest": request.expected_registry_parent_digest,
                "actual_registry_parent_digest": current_registry_digest,
            }),
        ));
    }

    validate_audit_against_request(&request.audit, &request, &current_registry_digest)?;
    if request.package_status == StrategyPackageStatus::Deprecated {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PackageDeprecated,
            "deprecated packages cannot be promoted into a champion registry",
            json!({
                "selection_key": request.selection_key,
                "package_digest": request.package_digest,
            }),
        ));
    }

    let audit_receipt_digest = canonical_digest(&request.audit)?;
    let receipt = StrategyPromotionReceipt {
        version: strategy_promotion_receipt_version().to_string(),
        registry_id: snapshot.registry_id.clone(),
        registry_version: request.registry_version.clone(),
        selection_key: request.selection_key.clone(),
        package_digest: request.package_digest.clone(),
        compatibility: request.compatibility,
        fixture_corpus_digest: request.fixture_corpus_digest.clone(),
        thresholds_digest: request.thresholds_digest.clone(),
        runner_policy_digest: request.runner_policy_digest.clone(),
        registry_parent_digest: current_registry_digest.clone(),
        audit_receipt_digest,
        audit_output_hash: request.audit.deterministic_output_hash.clone(),
    };

    let mut entries = snapshot.entries.clone();
    for entry in &mut entries {
        if entry.selection_key == request.selection_key
            && entry.champion_status == StrategyChampionStatus::Active
        {
            entry.champion_status = StrategyChampionStatus::Superseded;
        }
    }

    let entry = StrategyRegistryEntry {
        selection_key: request.selection_key,
        package_digest: request.package_digest,
        compatibility: request.compatibility,
        champion_status: StrategyChampionStatus::Active,
        package_status: request.package_status,
        promotion: receipt.clone(),
        audit: request.audit,
    };
    entries.push(entry.clone());

    let mut ancestry = snapshot.ancestry.clone();
    ancestry.push(current_registry_digest);

    let registry = StrategyRegistrySnapshot {
        version: strategy_registry_contract_version().to_string(),
        registry_id: snapshot.registry_id.clone(),
        registry_version: request.registry_version,
        ancestry,
        entries,
    };

    Ok(StrategyPromotionResult {
        registry,
        entry,
        receipt,
    })
}

pub fn deprecate(
    snapshot: &StrategyRegistrySnapshot,
    request: StrategyDeprecationRequest,
) -> StrategyLifecycleResult<StrategyRegistrySnapshot> {
    if request.registry_version == snapshot.registry_version {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::VersionBumpRequired,
            "deprecation requires an explicit new registry version",
            json!({
                "current_registry_version": snapshot.registry_version,
                "next_registry_version": request.registry_version,
            }),
        ));
    }

    let current_registry_digest = registry_snapshot_digest(snapshot)?;
    if request.expected_registry_parent_digest != current_registry_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::RegistryRace,
            "deprecation request was prepared against a different registry digest",
            json!({
                "expected_registry_parent_digest": request.expected_registry_parent_digest,
                "actual_registry_parent_digest": current_registry_digest,
            }),
        ));
    }

    let mut matched = 0usize;
    let mut entries = snapshot.entries.clone();
    for entry in &mut entries {
        if entry.selection_key == request.selection_key
            && entry.package_digest == request.package_digest
            && entry.champion_status == StrategyChampionStatus::Active
        {
            entry.champion_status = StrategyChampionStatus::Deprecated;
            entry.package_status = StrategyPackageStatus::Deprecated;
            matched += 1;
        }
    }

    match matched {
        0 => {
            return Err(StrategyLifecycleError::new(
                StrategyLifecycleErrorCode::NoActiveChampion,
                "no active champion matched the requested deprecation target",
                json!({
                    "selection_key": request.selection_key,
                    "package_digest": request.package_digest,
                }),
            ));
        }
        1 => {}
        _ => {
            return Err(StrategyLifecycleError::new(
                StrategyLifecycleErrorCode::DuplicateActiveChampion,
                "multiple active champions matched the requested deprecation target",
                json!({
                    "selection_key": request.selection_key,
                    "package_digest": request.package_digest,
                    "matches": matched,
                }),
            ));
        }
    }

    let mut ancestry = snapshot.ancestry.clone();
    ancestry.push(current_registry_digest);

    Ok(StrategyRegistrySnapshot {
        version: strategy_registry_contract_version().to_string(),
        registry_id: snapshot.registry_id.clone(),
        registry_version: request.registry_version,
        ancestry,
        entries,
    })
}

fn validate_audit_against_request(
    audit: &StrategyAuditReceipt,
    request: &StrategyPromotionRequest,
    current_registry_digest: &str,
) -> StrategyLifecycleResult<()> {
    if !audit.passed || audit.decision != StrategyAuditDecision::Proceed {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::AuditRejected,
            "only passing sealed audits may promote a strategy champion",
            json!({
                "selection_key": request.selection_key,
                "package_digest": request.package_digest,
                "decision": audit.decision,
                "passed": audit.passed,
            }),
        ));
    }

    if audit.registry_parent_digest != current_registry_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::StaleAudit,
            "audit receipt points at a stale registry parent digest",
            json!({
                "selection_key": request.selection_key,
                "audit_registry_parent_digest": audit.registry_parent_digest,
                "current_registry_digest": current_registry_digest,
            }),
        ));
    }

    if audit.selection_key != request.selection_key {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::SelectionKeyMismatch,
            "audit receipt selection key differs from the promotion request",
            json!({
                "audit_selection_key": audit.selection_key,
                "request_selection_key": request.selection_key,
            }),
        ));
    }

    if audit.package_digest != request.package_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::PackageDigestMismatch,
            "audit receipt package digest differs from the promotion request",
            json!({
                "audit_package_digest": audit.package_digest,
                "request_package_digest": request.package_digest,
            }),
        ));
    }

    if audit.fixture_corpus_digest != request.fixture_corpus_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::FixtureCorpusMismatch,
            "fixture corpus digest changed after the audit was sealed",
            json!({
                "audit_fixture_corpus_digest": audit.fixture_corpus_digest,
                "request_fixture_corpus_digest": request.fixture_corpus_digest,
            }),
        ));
    }

    if audit.thresholds_digest != request.thresholds_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::ThresholdMismatch,
            "promotion thresholds differ from the thresholds certified by audit",
            json!({
                "audit_thresholds_digest": audit.thresholds_digest,
                "request_thresholds_digest": request.thresholds_digest,
            }),
        ));
    }

    if audit.runner_policy_digest != request.runner_policy_digest {
        return Err(StrategyLifecycleError::new(
            StrategyLifecycleErrorCode::RunnerPolicyMismatch,
            "runner policy differs from the runner policy certified by audit",
            json!({
                "audit_runner_policy_digest": audit.runner_policy_digest,
                "request_runner_policy_digest": request.runner_policy_digest,
            }),
        ));
    }

    Ok(())
}
