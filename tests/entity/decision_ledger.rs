#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityArtifactReference, EntityInputReference,
        EntityPatchNamespaces, EntityProfileReference, EntityRegistrySnapshot,
        EntityStrategyReference,
        ledger::{
            DECISION_LEDGER_EVENT_VERSION, DecisionLedgerEvent, DecisionLedgerEventInput,
            DecisionLedgerEventType, DecisionLedgerExpectedContext, DecisionLedgerRefs,
            append_decision_ledger_event, build_decision_ledger_event,
            validate_decision_ledger_event, validate_decision_ledger_event_context,
        },
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn entity_decision_ledger_builds_stable_hashes_and_required_fields() {
    let fixture = ledger_fixture();
    let event = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_llc"),
        "operator@example.com",
        "2026-06-26T16:05:00Z",
        "blake3:previous-event",
        "review_merge_confirmed",
        "same tenant display label",
    ))
    .expect("ledger event builds");
    validate_decision_ledger_event(&event).expect("ledger event validates");

    assert_eq!(event.version, fixture.version);
    assert_eq!(event.event_version, DECISION_LEDGER_EVENT_VERSION);
    assert_eq!(event.event_version, fixture.event_version);
    assert_eq!(event.event_type, fixture.event_type);
    assert_eq!(event.decision, fixture.decision);
    assert_eq!(event.operator_id, fixture.operator_id);
    assert_eq!(event.source_artifact_hash, fixture.source_artifact_hash);
    assert_eq!(event.summary.counts, fixture.summary_counts);
    assert_eq!(event.summary.labels, fixture.summary_labels);
    assert!(event.decision_id.starts_with("decision:"));
    assert!(event.event_hash.starts_with("blake3:"));
    assert_eq!(event.artifact_content_hash, event.event_hash);
    assert_eq!(event.metadata.artifact_content_hash, event.event_hash);
    assert_eq!(event.metadata.profile.id, "cmbs_tenant_label");
    assert_eq!(event.metadata.profile.version, "0.1.0");
    assert_eq!(event.metadata.strategy.content_hash, "blake3:strategy");
    assert_eq!(
        event.metadata.registry_snapshot.lookup_snapshot_hash,
        "blake3:registry"
    );

    let rebuilt = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears_llc", "surf:sears"),
        "operator@example.com",
        "2026-06-26T16:05:00Z",
        "blake3:previous-event",
        "review_merge_confirmed",
        "same tenant display label",
    ))
    .expect("rebuilt event builds");
    assert_eq!(
        serde_json::to_vec(&event).expect("event serializes"),
        serde_json::to_vec(&rebuilt).expect("rebuilt event serializes")
    );

    let different_operator = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_llc"),
        "reviewer@example.com",
        "2026-06-26T16:05:00Z",
        "blake3:previous-event",
        "review_merge_confirmed",
        "same tenant display label",
    ))
    .expect("operator variant builds");
    assert_eq!(event.decision_id, different_operator.decision_id);
    assert_ne!(event.event_hash, different_operator.event_hash);

    assert_eq!(
        DecisionLedgerEventType::all()
            .iter()
            .map(|event_type| event_type.as_str())
            .collect::<Vec<_>>(),
        fixture
            .required_event_types
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
}

#[test]
fn decision_ledger_hashes_chain_append_only_jsonl_events() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path = temp_dir.path().join("decision-ledger.jsonl");
    let first = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_llc"),
        "operator@example.com",
        "2026-06-26T16:05:00Z",
        "blake3:previous-event",
        "review_merge_confirmed",
        "same tenant display label",
    ))
    .expect("first event builds");
    let first_receipt = append_decision_ledger_event(&path, &first).expect("first event appends");
    assert_eq!(first_receipt.decision_id, first.decision_id);
    assert_eq!(first_receipt.event_hash, first.event_hash);
    assert!(first_receipt.bytes_written > 0);

    let second = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::DistinctConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_auto"),
        "operator@example.com",
        "2026-06-26T16:06:00Z",
        &first.event_hash,
        "review_distinct_confirmed",
        "auto center is related but distinct tenant label",
    ))
    .expect("second event builds");
    append_decision_ledger_event(&path, &second).expect("second event appends");

    let contents = fs::read_to_string(&path).expect("ledger file reads");
    let events = contents
        .lines()
        .map(|line| serde_json::from_str::<DecisionLedgerEvent>(line).expect("ledger line parses"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], first);
    assert_eq!(events[1], second);
    assert_eq!(events[1].previous_event_hash, events[0].event_hash);
}

#[test]
fn decision_ledger_stale_source_hash_refuses_before_append() {
    let event = build_decision_ledger_event(sample_input(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_llc"),
        "operator@example.com",
        "2026-06-26T16:05:00Z",
        "blake3:previous-event",
        "review_merge_confirmed",
        "same tenant display label",
    ))
    .expect("event builds");
    let refusal = validate_decision_ledger_event_context(
        &event,
        &DecisionLedgerExpectedContext {
            profile_id: "cmbs_tenant_label".to_string(),
            profile_version: "0.1.0".to_string(),
            strategy_hash: "blake3:strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
            source_artifact_hash: "blake3:stale-solve".to_string(),
        },
    )
    .expect_err("stale context refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], "source_artifact_hash");
    assert_eq!(refusal.detail["expected"], "blake3:stale-solve");
    assert_eq!(refusal.detail["actual"], "blake3:solve");
    assert_eq!(refusal.detail["writes_performed"], false);
}

fn sample_input(
    event_type: DecisionLedgerEventType,
    refs: DecisionLedgerRefs,
    operator_id: &str,
    timestamp: &str,
    previous_event_hash: &str,
    reason_code: &str,
    note: &str,
) -> DecisionLedgerEventInput {
    DecisionLedgerEventInput {
        metadata: sample_metadata(),
        event_type,
        timestamp: timestamp.to_string(),
        operator_id: operator_id.to_string(),
        previous_event_hash: previous_event_hash.to_string(),
        source_artifact_hash: "blake3:solve".to_string(),
        refs,
        reason_code: reason_code.to_string(),
        note: note.to_string(),
    }
}

fn sample_metadata() -> EntityArtifactMetadata {
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
            row_count: 3,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![EntityArtifactReference {
            version: "canon_entity_solve.v0".to_string(),
            content_hash: "blake3:solve".to_string(),
        }],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn ledger_fixture() -> LedgerFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/ledger/merge_confirmed_expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[derive(Debug, Deserialize)]
struct LedgerFixture {
    version: String,
    event_version: String,
    event_type: DecisionLedgerEventType,
    decision: String,
    operator_id: String,
    source_artifact_hash: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    required_event_types: Vec<String>,
}
