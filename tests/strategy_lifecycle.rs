#![forbid(unsafe_code)]

#[path = "../src/strategy/explain.rs"]
mod explain;
#[path = "../src/strategy/promotion.rs"]
mod promotion;
#[path = "../src/strategy/registry.rs"]
mod registry;

use explain::{StrategyProjectImpactDisposition, explain as explain_strategy};
use promotion::{StrategyDeprecationRequest, StrategyPromotionRequest, deprecate, promote};
use registry::{
    StrategyAuditDecision, StrategyAuditReceipt, StrategyChampionStatus,
    StrategyCompatibilityLevel, StrategyLifecycleErrorCode, StrategyPackageStatus,
    StrategyRegistrySnapshot, StrategySelectionKey, canonical_digest, create_project_lock,
    registry_snapshot_digest, select_champion, strategy_audit_receipt_version, use_project_lock,
};

#[test]
fn promotion_selection_use_and_explain_bind_a_verifiable_chain() {
    let root = registry_root();
    let root_digest = registry_snapshot_digest(&root).expect("root digest");
    let key = task_key("sql_lineage");

    let promoted = promote(
        &root,
        StrategyPromotionRequest {
            registry_version: "0.2.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            compatibility: StrategyCompatibilityLevel::Exact,
            fixture_corpus_digest: "blake3:fixtures-v1".to_string(),
            thresholds_digest: "blake3:thresholds-v1".to_string(),
            runner_policy_digest: "blake3:runner-v1".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &root_digest,
                &key,
                "blake3:package-v1",
                "blake3:fixtures-v1",
                "blake3:thresholds-v1",
                "blake3:runner-v1",
            ),
            expected_registry_parent_digest: root_digest.clone(),
        },
    )
    .expect("promotion succeeds");

    assert_eq!(promoted.registry.ancestry, vec![root_digest.clone()]);

    let selection = select_champion(&promoted.registry, &key).expect("selection succeeds");
    assert_eq!(selection.package_digest, "blake3:package-v1");
    assert_eq!(selection.compatibility, StrategyCompatibilityLevel::Exact);

    let project_lock = create_project_lock("project-alpha", "blake3:project-alpha", &selection)
        .expect("lock succeeds");
    assert_eq!(project_lock.package_digest, "blake3:package-v1");

    let use_receipt = use_project_lock(&promoted.registry, &project_lock).expect("use succeeds");
    assert_eq!(use_receipt.package_digest, "blake3:package-v1");
    assert_eq!(
        use_receipt.audit_receipt_digest,
        selection.audit_receipt_digest
    );

    let explained =
        explain_strategy(&promoted.registry, &key, &[project_lock]).expect("explain succeeds");
    assert_eq!(explained.champion.package_digest, "blake3:package-v1");
    assert!(explained.alternatives.is_empty());
    assert_eq!(explained.project_impact.len(), 1);
    assert_eq!(
        explained.project_impact[0].disposition,
        StrategyProjectImpactDisposition::CurrentChampionPinned
    );
    assert_eq!(
        explained.champion.evidence.runner_policy_digest,
        "blake3:runner-v1"
    );
}

#[test]
fn lifecycle_refuses_stale_audits_changed_thresholds_altered_runners_and_registry_races() {
    let root = registry_root();
    let root_digest = registry_snapshot_digest(&root).expect("root digest");
    let key = task_key("sql_lineage");

    let first = promote(
        &root,
        StrategyPromotionRequest {
            registry_version: "0.2.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            compatibility: StrategyCompatibilityLevel::Exact,
            fixture_corpus_digest: "blake3:fixtures-v1".to_string(),
            thresholds_digest: "blake3:thresholds-v1".to_string(),
            runner_policy_digest: "blake3:runner-v1".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &root_digest,
                &key,
                "blake3:package-v1",
                "blake3:fixtures-v1",
                "blake3:thresholds-v1",
                "blake3:runner-v1",
            ),
            expected_registry_parent_digest: root_digest.clone(),
        },
    )
    .expect("first promotion succeeds");

    let current_digest = registry_snapshot_digest(&first.registry).expect("current digest");

    let stale_audit = promote(
        &first.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-v2".to_string(),
            runner_policy_digest: "blake3:runner-v2".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &root_digest,
                &key,
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: current_digest.clone(),
        },
    )
    .expect_err("stale audit refuses");
    assert_eq!(stale_audit.code, StrategyLifecycleErrorCode::StaleAudit);

    let changed_thresholds = promote(
        &first.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-mutated".to_string(),
            runner_policy_digest: "blake3:runner-v2".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &current_digest,
                &key,
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: current_digest.clone(),
        },
    )
    .expect_err("threshold mismatch refuses");
    assert_eq!(
        changed_thresholds.code,
        StrategyLifecycleErrorCode::ThresholdMismatch
    );

    let changed_runner = promote(
        &first.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-v2".to_string(),
            runner_policy_digest: "blake3:runner-mutated".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &current_digest,
                &key,
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: current_digest.clone(),
        },
    )
    .expect_err("runner mismatch refuses");
    assert_eq!(
        changed_runner.code,
        StrategyLifecycleErrorCode::RunnerPolicyMismatch
    );

    let race = promote(
        &first.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-v2".to_string(),
            runner_policy_digest: "blake3:runner-v2".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &current_digest,
                &key,
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: root_digest,
        },
    )
    .expect_err("registry race refuses");
    assert_eq!(race.code, StrategyLifecycleErrorCode::RegistryRace);

    let deprecated_package = promote(
        &first.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key,
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-v2".to_string(),
            runner_policy_digest: "blake3:runner-v2".to_string(),
            package_status: StrategyPackageStatus::Deprecated,
            audit: sealed_audit(
                &current_digest,
                &task_key("sql_lineage"),
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: current_digest,
        },
    )
    .expect_err("deprecated package refuses");
    assert_eq!(
        deprecated_package.code,
        StrategyLifecycleErrorCode::PackageDeprecated
    );
}

#[test]
fn lifecycle_supports_rollback_by_new_version_and_deprecated_champions() {
    let root = registry_root();
    let key = task_key("sql_lineage");
    let root_digest = registry_snapshot_digest(&root).expect("root digest");

    let v1 = promote(
        &root,
        StrategyPromotionRequest {
            registry_version: "0.2.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            compatibility: StrategyCompatibilityLevel::Exact,
            fixture_corpus_digest: "blake3:fixtures-v1".to_string(),
            thresholds_digest: "blake3:thresholds-v1".to_string(),
            runner_policy_digest: "blake3:runner-v1".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &root_digest,
                &key,
                "blake3:package-v1",
                "blake3:fixtures-v1",
                "blake3:thresholds-v1",
                "blake3:runner-v1",
            ),
            expected_registry_parent_digest: root_digest,
        },
    )
    .expect("v1 promotion succeeds");
    let v1_digest = registry_snapshot_digest(&v1.registry).expect("v1 digest");

    let v2 = promote(
        &v1.registry,
        StrategyPromotionRequest {
            registry_version: "0.3.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v2".to_string(),
            compatibility: StrategyCompatibilityLevel::Compatible,
            fixture_corpus_digest: "blake3:fixtures-v2".to_string(),
            thresholds_digest: "blake3:thresholds-v2".to_string(),
            runner_policy_digest: "blake3:runner-v2".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &v1_digest,
                &key,
                "blake3:package-v2",
                "blake3:fixtures-v2",
                "blake3:thresholds-v2",
                "blake3:runner-v2",
            ),
            expected_registry_parent_digest: v1_digest.clone(),
        },
    )
    .expect("v2 promotion succeeds");
    let v2_digest = registry_snapshot_digest(&v2.registry).expect("v2 digest");

    let rollback = promote(
        &v2.registry,
        StrategyPromotionRequest {
            registry_version: "0.4.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            compatibility: StrategyCompatibilityLevel::Exact,
            fixture_corpus_digest: "blake3:fixtures-v1".to_string(),
            thresholds_digest: "blake3:thresholds-v1".to_string(),
            runner_policy_digest: "blake3:runner-v1".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &v2_digest,
                &key,
                "blake3:package-v1",
                "blake3:fixtures-v1",
                "blake3:thresholds-v1",
                "blake3:runner-v1",
            ),
            expected_registry_parent_digest: v2_digest.clone(),
        },
    )
    .expect("rollback-by-new-version succeeds");

    let rollback_selection =
        select_champion(&rollback.registry, &key).expect("rollback selection succeeds");
    assert_eq!(rollback_selection.package_digest, "blake3:package-v1");
    assert_eq!(rollback_selection.alternatives.len(), 2);
    assert!(rollback_selection.alternatives.iter().any(|entry| {
        entry.package_digest == "blake3:package-v2"
            && entry.champion_status == StrategyChampionStatus::Superseded
    }));

    let rollback_lock =
        create_project_lock("project-beta", "blake3:project-beta", &rollback_selection)
            .expect("rollback lock");

    let rollback_digest = registry_snapshot_digest(&rollback.registry).expect("rollback digest");
    let deprecated = deprecate(
        &rollback.registry,
        StrategyDeprecationRequest {
            registry_version: "0.5.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            expected_registry_parent_digest: rollback_digest,
        },
    )
    .expect("deprecation succeeds");

    let no_active = select_champion(&deprecated, &key).expect_err("deprecated champion refuses");
    assert_eq!(no_active.code, StrategyLifecycleErrorCode::NoActiveChampion);

    let stale_use = use_project_lock(&deprecated, &rollback_lock).expect_err("stale lock refuses");
    assert_eq!(stale_use.code, StrategyLifecycleErrorCode::ProjectLockStale);
}

#[test]
fn tampered_chain_and_selection_proof_are_refused_before_use() {
    let root = registry_root();
    let key = task_key("sql_lineage");
    let root_digest = registry_snapshot_digest(&root).expect("root digest");

    let promoted = promote(
        &root,
        StrategyPromotionRequest {
            registry_version: "0.2.0".to_string(),
            selection_key: key.clone(),
            package_digest: "blake3:package-v1".to_string(),
            compatibility: StrategyCompatibilityLevel::Exact,
            fixture_corpus_digest: "blake3:fixtures-v1".to_string(),
            thresholds_digest: "blake3:thresholds-v1".to_string(),
            runner_policy_digest: "blake3:runner-v1".to_string(),
            package_status: StrategyPackageStatus::Available,
            audit: sealed_audit(
                &root_digest,
                &key,
                "blake3:package-v1",
                "blake3:fixtures-v1",
                "blake3:thresholds-v1",
                "blake3:runner-v1",
            ),
            expected_registry_parent_digest: root_digest,
        },
    )
    .expect("promotion succeeds");

    let selection = select_champion(&promoted.registry, &key).expect("selection succeeds");
    let mut lock =
        create_project_lock("project-gamma", "blake3:project-gamma", &selection).unwrap();

    let mut tampered_registry = promoted.registry.clone();
    tampered_registry.entries[0].audit.thresholds_digest = "blake3:tampered-thresholds".to_string();
    let tampered_chain =
        select_champion(&tampered_registry, &key).expect_err("tampered chain refuses");
    assert_eq!(
        tampered_chain.code,
        StrategyLifecycleErrorCode::ThresholdMismatch
    );

    lock.selection_proof_digest = "blake3:tampered-proof".to_string();
    let stale_proof =
        use_project_lock(&promoted.registry, &lock).expect_err("tampered proof refuses");
    assert_eq!(
        stale_proof.code,
        StrategyLifecycleErrorCode::SelectionProofStale
    );

    let mut deprecated_package_registry = promoted.registry.clone();
    deprecated_package_registry.entries[0].package_status = StrategyPackageStatus::Deprecated;
    let mut deprecated_lock =
        create_project_lock("project-delta", "blake3:project-delta", &selection)
            .expect("delta lock");
    deprecated_lock.registry_digest =
        registry_snapshot_digest(&deprecated_package_registry).expect("deprecated digest");
    let deprecated_use = use_project_lock(&deprecated_package_registry, &deprecated_lock)
        .expect_err("deprecated package refuses use");
    assert_eq!(
        deprecated_use.code,
        StrategyLifecycleErrorCode::PackageDeprecated
    );
}

fn registry_root() -> StrategyRegistrySnapshot {
    StrategyRegistrySnapshot::empty("strategy-registry", "0.1.0")
}

fn task_key(task: &str) -> StrategySelectionKey {
    StrategySelectionKey::TaskTransform {
        task: task.to_string(),
        skill_hash: "blake3:skill-sql-lineage".to_string(),
    }
}

fn sealed_audit(
    registry_parent_digest: &str,
    selection_key: &StrategySelectionKey,
    package_digest: &str,
    fixture_corpus_digest: &str,
    thresholds_digest: &str,
    runner_policy_digest: &str,
) -> StrategyAuditReceipt {
    let receipt = StrategyAuditReceipt {
        version: strategy_audit_receipt_version().to_string(),
        selection_key: selection_key.clone(),
        package_digest: package_digest.to_string(),
        fixture_corpus_digest: fixture_corpus_digest.to_string(),
        thresholds_digest: thresholds_digest.to_string(),
        runner_policy_digest: runner_policy_digest.to_string(),
        registry_parent_digest: registry_parent_digest.to_string(),
        deterministic_output_hash: deterministic_output_hash(
            selection_key,
            package_digest,
            fixture_corpus_digest,
            thresholds_digest,
            runner_policy_digest,
        ),
        passed: true,
        decision: StrategyAuditDecision::Proceed,
    };
    assert!(canonical_digest(&receipt).unwrap().starts_with("blake3:"));
    receipt
}

fn deterministic_output_hash(
    selection_key: &StrategySelectionKey,
    package_digest: &str,
    fixture_corpus_digest: &str,
    thresholds_digest: &str,
    runner_policy_digest: &str,
) -> String {
    canonical_digest(&serde_json::json!({
        "selection_key": selection_key,
        "package_digest": package_digest,
        "fixture_corpus_digest": fixture_corpus_digest,
        "thresholds_digest": thresholds_digest,
        "runner_policy_digest": runner_policy_digest,
    }))
    .expect("hashes")
}
