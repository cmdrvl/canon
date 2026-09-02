use canon::inbox::capture::{
    CaptureContext, CaptureRequest, SOURCE_ARTIFACT_HASH_NAMESPACE, canonical_capture_bytes,
    capture_artifact, capture_exact_mapping_artifact, merge_capture_shards, write_capture_artifact,
};
use canon::inbox::{
    CandidateStatus, InboxErrorCode, InboxEventKind, InboxExportMode, InboxFieldRole,
    InboxPrivacyPolicy, InboxReasonCode, RawValueRetention,
};
use serde_json::json;
use std::fs;

fn digest(byte: u8) -> String {
    format!("blake3:{}", char::from(byte).to_string().repeat(64))
}

fn policy() -> InboxPrivacyPolicy {
    InboxPrivacyPolicy {
        policy_id: "test-policy".to_string(),
        raw_value_retention: RawValueRetention::Omit,
        default_export_mode: InboxExportMode::Redacted,
        merge_mode: Default::default(),
    }
}

fn request(source_ref: &str, source_hash: String) -> CaptureRequest {
    let mut context = CaptureContext::new(
        "project-alpha",
        "run-001",
        source_ref,
        source_hash,
        "2026-09-02T12:00:00Z",
        "cusip",
        InboxFieldRole::LookupInput,
    );
    context.namespace_hints.push(canon::inbox::NamespaceHint {
        namespace: "registry".to_string(),
        source: "registries/cusip-isin".to_string(),
    });
    CaptureRequest::new(policy(), InboxExportMode::Redacted, context)
}

#[test]
fn exact_lookup_capture_maps_unresolved_reasons_and_skips_resolved_rows() {
    let upstream = json!({
        "version": "canon.v0",
        "outcome": "PARTIAL",
        "mappings": [
            {"input": "037833100", "canonical_id": "US0378331005"}
        ],
        "unresolved": [
            {"input": "UNKNOWN", "reason": "no_matching_rule", "record_ref": "row-2"},
            {"input": "", "reason": "empty_value", "record_ref": "row-3"},
            {"input": null, "reason": "missing_field", "record_ref": "row-4"}
        ]
    });

    let inbox = capture_exact_mapping_artifact(&upstream, &request("exact-map.json", digest(b'a')))
        .expect("exact unresolved values become inbox events");
    assert_eq!(inbox.version, "canon.unresolved.inbox.v1");
    assert_eq!(inbox.summary.total_items, 3);
    assert_eq!(inbox.summary.total_occurrences, 3);
    assert_eq!(inbox.summary.by_reason_code["no_matching_rule"], 1);
    assert_eq!(inbox.summary.by_reason_code["empty_value"], 1);
    assert_eq!(inbox.summary.by_reason_code["missing_field"], 1);
    assert_eq!(inbox.summary.by_event_kind["exact_lookup"], 3);
    assert!(inbox.items.iter().all(|item| item.raw_values.is_empty()));
    assert!(inbox.items.iter().any(|item| {
        item.reason_code == InboxReasonCode::NoMatchingRule
            && item.event_kind == InboxEventKind::ExactLookup
            && item.namespace_hints.iter().any(|hint| {
                hint.namespace == SOURCE_ARTIFACT_HASH_NAMESPACE && hint.source == digest(b'a')
            })
    }));

    let repeated =
        capture_exact_mapping_artifact(&upstream, &request("exact-map.json", digest(b'a')))
            .expect("same capture replays");
    assert_eq!(
        canonical_capture_bytes(&inbox).expect("canonical bytes"),
        canonical_capture_bytes(&repeated).expect("canonical bytes"),
        "same input and explicit source context must produce byte-identical inbox output"
    );
}

#[test]
fn exact_lookup_refusals_resolved_outputs_and_unknown_reasons_are_not_captured() {
    for upstream in [
        json!({"version": "canon.v0", "outcome": "RESOLVED", "mappings": []}),
        json!({"version": "canon.v0", "outcome": "REFUSAL", "code": "E_BAD_REGISTRY"}),
    ] {
        let inbox = capture_exact_mapping_artifact(&upstream, &request("empty.json", digest(b'b')))
            .expect("resolved outputs and refusals are not unresolved evidence");
        assert_eq!(inbox.summary.total_items, 0);
        assert!(inbox.items.is_empty());
    }

    let malformed = json!({
        "version": "canon.v0",
        "outcome": "UNRESOLVED",
        "unresolved": [{"input": "X", "reason": "operator_policy_refusal"}]
    });
    let error = capture_exact_mapping_artifact(&malformed, &request("bad.json", digest(b'c')))
        .expect_err("unknown exact reasons must not be silently reclassified");
    assert_eq!(error.code, InboxErrorCode::ArtifactContract);
}

#[test]
fn entity_provider_and_project_workflows_emit_stable_outcome_reason_codes() {
    let entity = json!({
        "version": "canon_entity_link.v1",
        "ambiguous": [
            {
                "record_ref": "entity-row-1",
                "field_name": "issuer_name",
                "input": "ACME",
                "candidate_ids": ["ENT-001", "ENT-002"]
            }
        ],
        "contradictions": [
            {
                "record_ref": "entity-row-2",
                "field_name": "issuer_name",
                "surface": "ACME HOLDCO",
                "reason": "hard contradiction between anchors"
            }
        ],
        "review_deferred": [
            {
                "record_ref": "entity-row-3",
                "field_name": "issuer_name",
                "surface": "ACME LLC",
                "reason": "score below review threshold"
            }
        ],
        "items": [
            {"state": "resolved", "record_ref": "entity-row-4", "surface": "ACME INC"}
        ]
    });
    let provider = json!({
        "version": "canon_registry_build.v0",
        "failures": [
            {
                "identifier": "037833100",
                "provider": "fixture-provider",
                "reason": "provider conflict across retained evidence"
            }
        ]
    });
    let project = json!({
        "schema_version": "canon.project.run.v2",
        "node_receipts": [
            {
                "node_id": "solve-identity",
                "outcome": "failed",
                "failure_code": "E_ENTITY_CANDIDATE_BUDGET",
                "failure_message": "candidate budget exhausted"
            },
            {
                "node_id": "bad-input",
                "outcome": "failed",
                "failure_code": "E_COLUMN_NOT_FOUND",
                "failure_message": "invalid operator input"
            },
            {
                "node_id": "already-ok",
                "outcome": "succeeded"
            }
        ]
    });

    let merged = merge_capture_shards([
        capture_artifact(&entity, &request("entity.json", digest(b'd'))).expect("entity capture"),
        capture_artifact(&provider, &request("provider.json", digest(b'e')))
            .expect("provider capture"),
        capture_artifact(&project, &request("project.json", digest(b'f')))
            .expect("project receipt capture"),
    ])
    .expect("workflow shards merge deterministically");

    assert_eq!(merged.summary.total_items, 5);
    assert_eq!(merged.summary.by_reason_code["ambiguous_candidates"], 1);
    assert_eq!(merged.summary.by_reason_code["cannot_link"], 2);
    assert_eq!(merged.summary.by_reason_code["score_below_threshold"], 1);
    assert_eq!(merged.summary.by_reason_code["budget_exceeded"], 1);
    assert!(merged.items.iter().any(|item| item.reason_code
        == InboxReasonCode::AmbiguousCandidates
        && item.candidate_summary.status == CandidateStatus::Ambiguous
        && item.candidate_summary.candidate_count == 2));
    assert!(
        !merged.items.iter().any(|item| item
            .occurrences
            .iter()
            .any(|occurrence| occurrence.record_ref.as_deref() == Some("bad-input"))),
        "invalid input receipts are not unresolved identity evidence"
    );
}

#[test]
fn duplicate_replay_and_independent_shard_merge_are_deterministic() {
    let upstream = json!({
        "version": "canon.v0",
        "outcome": "UNRESOLVED",
        "unresolved": [
            {"input": "UNKNOWN", "reason": "no_matching_rule", "record_ref": "row-1"}
        ]
    });
    let first = capture_artifact(&upstream, &request("a.json", digest(b'a'))).expect("capture");
    let duplicate = capture_artifact(&upstream, &request("a.json", digest(b'a'))).expect("capture");
    let deduped = merge_capture_shards([first.clone(), duplicate]).expect("dedupe duplicate");
    assert_eq!(deduped.summary.total_items, 1);
    assert_eq!(deduped.summary.total_occurrences, 1);

    let second = capture_artifact(&upstream, &request("b.json", digest(b'b'))).expect("capture");
    let forward = merge_capture_shards([first.clone(), second.clone()]).expect("forward merge");
    let reverse = merge_capture_shards([second, first]).expect("reverse merge");
    assert_eq!(
        canonical_capture_bytes(&forward).expect("canonical bytes"),
        canonical_capture_bytes(&reverse).expect("canonical bytes"),
        "independent shard merge order must not affect bytes"
    );
    assert_eq!(forward.summary.total_items, 1);
    assert_eq!(forward.summary.total_occurrences, 2);
}

#[test]
fn read_only_bytes_do_not_write_and_explicit_output_is_create_new() {
    let upstream = json!({
        "version": "canon.v0",
        "outcome": "UNRESOLVED",
        "unresolved": [
            {"input": "UNKNOWN", "reason": "no_matching_rule", "record_ref": "row-1"}
        ]
    });
    let inbox = capture_artifact(&upstream, &request("exact.json", digest(b'a'))).expect("capture");
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("inbox.json");

    let bytes = canonical_capture_bytes(&inbox).expect("canonical bytes");
    assert!(
        !output.exists(),
        "read-only canonical byte emission must not mutate ambient paths"
    );

    write_capture_artifact(&output, &inbox).expect("explicit write succeeds");
    assert_eq!(fs::read(&output).expect("read output"), bytes);
    let before = fs::read(&output).expect("read output before refused overwrite");
    let error =
        write_capture_artifact(&output, &inbox).expect_err("capture output is create-new only");
    assert_eq!(error.code, InboxErrorCode::ArtifactContract);
    assert_eq!(
        fs::read(&output).expect("read output after refused overwrite"),
        before
    );
}
