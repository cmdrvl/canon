#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        ledger::{DecisionLedgerEvent, DecisionLedgerExpectedContext},
        ledger_replay::{
            DecisionLedgerReplayRequest, parse_decision_ledger_jsonl, replay_decision_ledger,
        },
        patches::{ReviewPatchBundle, derive_review_patches},
        review_import::{
            ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
            import_review_decisions, parse_review_import_json,
        },
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[test]
fn entity_ledger_golden_import_replay_and_patch_derivation_is_stable() {
    let decisions = import_decisions();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");

    let receipt = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions,
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:45:00Z".to_string(),
        previous_event_hash: "blake3:ledger-start".to_string(),
    })
    .expect("review import appends ledger events");

    assert_eq!(receipt.accepted_decisions, 2);
    let ledger_jsonl = fs::read_to_string(&ledger_path).expect("ledger jsonl reads");
    let events = parse_decision_ledger_jsonl(&ledger_jsonl).expect("ledger jsonl parses");
    assert_ledger_chain(&events, "blake3:ledger-start", &receipt.last_event_hash);

    let report = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context(),
        starting_previous_event_hash: "blake3:ledger-start".to_string(),
        events,
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect("ledger replays");
    let patches = derive_review_patches(&report).expect("review patches derive");

    let actual = LedgerGolden::from_parts(receipt.accepted_decisions, &ledger_jsonl, &patches);
    let expected = ledger_golden();
    assert_eq!(actual, expected);
}

#[test]
fn entity_ledger_golden_stale_review_import_refuses_without_append() {
    let mut decisions = import_decisions();
    decisions[0].source_review_queue_hash = "blake3:stale-review-queue".to_string();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");

    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions,
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:45:00Z".to_string(),
        previous_event_hash: "blake3:ledger-start".to_string(),
    })
    .expect_err("stale review import refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], "source_review_queue_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_no_ledger_append(&ledger_path);
}

#[test]
fn entity_ledger_golden_duplicate_decisions_refuse_without_append() {
    let mut decisions = import_decisions();
    decisions.push(decisions[0].clone());
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");

    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions,
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:45:00Z".to_string(),
        previous_event_hash: "blake3:ledger-start".to_string(),
    })
    .expect_err("duplicate review decisions refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], "review_id");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_no_ledger_append(&ledger_path);
}

fn import_decisions() -> Vec<ReviewImportDecision> {
    parse_review_import_json(include_str!(
        "../fixtures/entity/review/golden/operator_decisions.json"
    ))
    .expect("operator decisions parse")
}

fn import_context() -> ReviewImportContext {
    ReviewImportContext {
        metadata: metadata(),
        source_review_queue_hash: "blake3:review-queue-golden".to_string(),
        known_review_ids: BTreeSet::from([
            "review:surf_sears".to_string(),
            "review:surf_pnc_bank".to_string(),
        ]),
        cannot_link_review_ids: BTreeSet::from(["review:surf_sears".to_string()]),
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 153,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn expected_context() -> DecisionLedgerExpectedContext {
    DecisionLedgerExpectedContext {
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        source_artifact_hash: "blake3:review-queue-golden".to_string(),
    }
}

fn assert_ledger_chain(
    events: &[DecisionLedgerEvent],
    starting_previous_event_hash: &str,
    receipt_last_event_hash: &str,
) {
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].previous_event_hash, starting_previous_event_hash);
    assert_eq!(events[1].previous_event_hash, events[0].event_hash);
    assert_eq!(receipt_last_event_hash, events[1].event_hash);
    for event in events {
        assert!(event.decision_id.starts_with("decision:"));
        assert!(event.event_hash.starts_with("blake3:"));
        assert_eq!(event.metadata.profile.id, "cmbs_tenant_label");
        assert_eq!(
            event.metadata.registry_snapshot.lookup_snapshot_hash,
            "blake3:registry"
        );
    }
}

fn assert_no_ledger_append(path: &Path) {
    assert!(!path.exists(), "ledger must not be created on refusal");
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LedgerGolden {
    accepted_decisions: u64,
    ledger_event_types: Vec<String>,
    ledger_reason_codes: Vec<String>,
    replay_summary_counts: BTreeMap<String, u64>,
    patch_projection: PatchProjection,
}

impl LedgerGolden {
    fn from_parts(
        accepted_decisions: u64,
        ledger_jsonl: &str,
        patches: &ReviewPatchBundle,
    ) -> Self {
        let events = parse_decision_ledger_jsonl(ledger_jsonl).expect("ledger jsonl parses");
        let report = replay_decision_ledger(DecisionLedgerReplayRequest {
            expected_context: expected_context(),
            starting_previous_event_hash: "blake3:ledger-start".to_string(),
            events: events.clone(),
            cannot_link_override_decision_ids: BTreeSet::new(),
        })
        .expect("ledger replays for projection");

        Self {
            accepted_decisions,
            ledger_event_types: events
                .iter()
                .map(|event| event.event_type.as_str().to_string())
                .collect(),
            ledger_reason_codes: events
                .iter()
                .map(|event| event.reason_code.clone())
                .collect(),
            replay_summary_counts: report.summary.counts,
            patch_projection: PatchProjection::from_bundle(patches),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PatchProjection {
    aliases: usize,
    distinct: Vec<PairPatchGolden>,
    cannot_link: Vec<CannotLinkGolden>,
    relations: Vec<PairPatchGolden>,
    overrides: usize,
}

impl PatchProjection {
    fn from_bundle(bundle: &ReviewPatchBundle) -> Self {
        Self {
            aliases: bundle.alias_patches.len(),
            distinct: bundle
                .distinct_patches
                .iter()
                .map(|patch| PairPatchGolden {
                    profile_id: patch.profile_id.clone(),
                    identity_semantics: patch.identity_semantics.clone(),
                    namespace: patch.namespace.clone(),
                    left: patch.left.clone(),
                    right: patch.right.clone(),
                    reason: patch.reason.clone(),
                })
                .collect(),
            cannot_link: bundle
                .cannot_link_sidecars
                .iter()
                .map(|sidecar| CannotLinkGolden {
                    profile_id: sidecar.profile_id.clone(),
                    identity_semantics: sidecar.identity_semantics.clone(),
                    left: sidecar.left.clone(),
                    right: sidecar.right.clone(),
                    hard_cannot_link: sidecar.hard_cannot_link,
                    reason: sidecar.reason.clone(),
                })
                .collect(),
            relations: bundle
                .relation_patches
                .iter()
                .map(|patch| PairPatchGolden {
                    profile_id: patch.profile_id.clone(),
                    identity_semantics: patch.identity_semantics.clone(),
                    namespace: patch.namespace.clone(),
                    left: patch.left.clone(),
                    right: patch.right.clone(),
                    reason: patch.relation.clone(),
                })
                .collect(),
            overrides: bundle.override_records.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PairPatchGolden {
    profile_id: String,
    identity_semantics: String,
    namespace: String,
    left: String,
    right: String,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CannotLinkGolden {
    profile_id: String,
    identity_semantics: String,
    left: String,
    right: String,
    hard_cannot_link: bool,
    reason: String,
}

fn ledger_golden() -> LedgerGolden {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/golden/ledger_e2e_expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}
