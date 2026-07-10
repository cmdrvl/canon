#![forbid(unsafe_code)]

use canon::inbox::{
    CANON_UNRESOLVED_INBOX_VERSION, CandidateStatus, ExternalRawValueReference, InboxErrorCode,
    InboxEventKind, InboxExportMode, InboxFieldRole, InboxOccurrenceRef, InboxPrivacyPolicy,
    InboxReasonCode, NamespaceHint, NormalizedSurfaceFingerprint, PrivacyClass, ProfileFieldRef,
    RawValueRetention, TemporalScope, UnresolvedInboxArtifact, UnresolvedInboxItem,
    canonical_json_bytes, export_artifact, finalize_artifact, merge_artifacts,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.unresolved.inbox.v1.schema.json");

#[test]
fn schema_declares_non_registry_deterministic_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.unresolved.inbox.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_UNRESOLVED_INBOX_VERSION
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("not a shadow registry")
    );
}

#[test]
fn retained_artifact_round_trips() {
    let artifact = finalize_artifact(sample_artifact(InboxExportMode::Retained))
        .expect("retained artifact finalizes");
    let json = serde_json::to_string(&artifact).expect("artifact serializes");
    let round_tripped: UnresolvedInboxArtifact =
        serde_json::from_str(&json).expect("artifact deserializes");
    assert_eq!(round_tripped, artifact);
    assert_eq!(artifact.summary.retained_raw_reference_count, 1);
    assert!(!artifact.items[0].raw_values_redacted);
}

#[test]
fn redacted_export_round_trips() {
    let retained = finalize_artifact(sample_artifact(InboxExportMode::Retained))
        .expect("retained artifact finalizes");
    let redacted =
        export_artifact(&retained, InboxExportMode::Redacted).expect("redacted export succeeds");
    let json = serde_json::to_string(&redacted).expect("redacted artifact serializes");
    let round_tripped: UnresolvedInboxArtifact =
        serde_json::from_str(&json).expect("redacted artifact deserializes");
    assert_eq!(round_tripped, redacted);
    assert!(redacted.items[0].raw_values.is_empty());
    assert!(redacted.items[0].raw_values_redacted);
    assert_eq!(redacted.summary.redacted_items, 1);
}

#[test]
fn merge_reordered_shards_is_deterministic() {
    let first = finalize_artifact(sample_shard(
        "proj-a",
        "run-2",
        "source-a",
        "2026-07-10T10:05:00-04:00",
        vec![NamespaceHint {
            namespace: "cusip".to_string(),
            source: "column".to_string(),
        }],
    ))
    .expect("first shard finalizes");
    let second = finalize_artifact(sample_shard(
        "proj-b",
        "run-1",
        "source-b",
        "2026-07-09T23:59:59Z",
        vec![NamespaceHint {
            namespace: "isin".to_string(),
            source: "profile".to_string(),
        }],
    ))
    .expect("second shard finalizes");

    let merged_a = merge_artifacts(vec![first.clone(), second.clone()]).expect("merge a succeeds");
    let merged_b = merge_artifacts(vec![second, first]).expect("merge b succeeds");

    assert_eq!(
        canonical_json_bytes(&merged_a).unwrap(),
        canonical_json_bytes(&merged_b).unwrap()
    );
    assert_eq!(merged_a.summary.total_items, 1);
    assert_eq!(merged_a.summary.total_occurrences, 2);
    assert_eq!(merged_a.items[0].occurrence_summary.distinct_projects, 2);
    assert_eq!(merged_a.items[0].first_seen_at, "2026-07-09T23:59:59Z");
    assert_eq!(merged_a.items[0].last_seen_at, "2026-07-10T14:05:00Z");
    assert_eq!(merged_a.items[0].namespace_hints.len(), 2);
}

#[test]
fn unicode_contract_round_trips() {
    let mut artifact = sample_artifact(InboxExportMode::Retained);
    artifact.items[0].field_name = "issuer_名称".to_string();
    artifact.items[0].surface_fingerprints = vec![
        sample_fingerprint("alias", "blake3:33333333333333333333333333333333"),
        sample_fingerprint("primary", "blake3:22222222222222222222222222222222"),
    ];
    artifact.items[0].profile_ref = Some(ProfileFieldRef {
        profile_id: "sec10d_租户".to_string(),
        profile_version: "1.2.0".to_string(),
    });
    artifact.items[0].privacy_class = Some(PrivacyClass::Restricted);

    let artifact = finalize_artifact(artifact).expect("unicode artifact finalizes");
    let json = serde_json::to_string(&artifact).expect("unicode artifact serializes");
    let round_tripped: UnresolvedInboxArtifact =
        serde_json::from_str(&json).expect("unicode artifact deserializes");
    assert_eq!(round_tripped, artifact);
    assert_eq!(artifact.items[0].field_name, "issuer_名称");
    assert_eq!(artifact.summary.by_privacy_class["restricted"], 1);
}

#[test]
fn incompatible_privacy_policy_is_rejected() {
    let retained = finalize_artifact(sample_artifact(InboxExportMode::Retained))
        .expect("retained artifact finalizes");
    let mut other = sample_artifact(InboxExportMode::Retained);
    other.policy.policy_id = "policy.other".to_string();
    let other = finalize_artifact(other).expect("other artifact finalizes");

    let error = merge_artifacts(vec![retained.clone(), other]).expect_err("merge should fail");
    assert_eq!(error.code, InboxErrorCode::PrivacyPolicy);

    let redacted =
        export_artifact(&retained, InboxExportMode::Redacted).expect("redacted export succeeds");
    let error = export_artifact(&redacted, InboxExportMode::Retained)
        .expect_err("retained export should not be recoverable");
    assert_eq!(error.code, InboxErrorCode::PrivacyPolicy);
}

#[test]
fn corrupt_reference_is_rejected() {
    let mut artifact = sample_artifact(InboxExportMode::Retained);
    artifact.items[0].raw_values[0].content_hash = "sha256:bad".to_string();
    let error = finalize_artifact(artifact).expect_err("bad raw reference should fail");
    assert_eq!(error.code, InboxErrorCode::CorruptReference);
}

#[test]
fn artifact_bytes_are_stable() {
    let first = finalize_artifact(sample_artifact(InboxExportMode::Retained))
        .expect("first artifact finalizes");
    let second = finalize_artifact(sample_artifact(InboxExportMode::Retained))
        .expect("second artifact finalizes");
    assert_eq!(
        canonical_json_bytes(&first).unwrap(),
        canonical_json_bytes(&second).unwrap()
    );
    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
}

fn sample_artifact(view: InboxExportMode) -> UnresolvedInboxArtifact {
    let mut item = sample_item();
    if matches!(view, InboxExportMode::Redacted) {
        item.raw_values.clear();
        item.raw_values_redacted = true;
    }
    if matches!(view, InboxExportMode::FingerprintsOnly) {
        item.raw_values.clear();
        item.raw_values_redacted = false;
    }

    UnresolvedInboxArtifact {
        version: CANON_UNRESOLVED_INBOX_VERSION.to_string(),
        view,
        artifact_content_hash: String::new(),
        policy: InboxPrivacyPolicy {
            policy_id: "policy.default".to_string(),
            raw_value_retention: RawValueRetention::ExternalReference,
            default_export_mode: view,
            merge_mode: canon::inbox::InboxMergeMode::Strict,
        },
        summary: canon::inbox::InboxSummary::default(),
        items: vec![item],
    }
}

fn sample_shard(
    project_ref: &str,
    run_ref: &str,
    source_ref: &str,
    seen_at: &str,
    namespace_hints: Vec<NamespaceHint>,
) -> UnresolvedInboxArtifact {
    let mut artifact = sample_artifact(InboxExportMode::Redacted);
    artifact.items[0].namespace_hints = namespace_hints;
    artifact.items[0].occurrences = vec![InboxOccurrenceRef {
        project_ref: project_ref.to_string(),
        run_ref: run_ref.to_string(),
        source_ref: source_ref.to_string(),
        record_ref: Some(format!("{project_ref}:{source_ref}:row-1")),
        seen_at: seen_at.to_string(),
    }];
    artifact.items[0].first_seen_at.clear();
    artifact.items[0].last_seen_at.clear();
    artifact
}

fn sample_item() -> UnresolvedInboxItem {
    UnresolvedInboxItem {
        event_key: String::new(),
        event_kind: InboxEventKind::ExactLookup,
        reason_code: InboxReasonCode::NoMatchingRule,
        field_name: "issuer_name".to_string(),
        field_role: InboxFieldRole::LookupInput,
        profile_ref: Some(ProfileFieldRef {
            profile_id: "sec10d_issuer".to_string(),
            profile_version: "1.0.0".to_string(),
        }),
        surface_fingerprints: vec![
            sample_fingerprint("alias", "blake3:11111111111111111111111111111111"),
            sample_fingerprint("primary", "blake3:00000000000000000000000000000000"),
        ],
        namespace_hints: vec![NamespaceHint {
            namespace: "issuer_name".to_string(),
            source: "column".to_string(),
        }],
        candidate_summary: canon::inbox::CandidateSummary {
            status: CandidateStatus::Rejected,
            candidate_count: 2,
            best_score_band: Some("0.80-0.89".to_string()),
            rejection_reasons: vec![
                "protected_conflict".to_string(),
                "below_accept_threshold".to_string(),
            ],
        },
        temporal_scope: Some(TemporalScope {
            start_at: Some("2026-07-01T00:00:00Z".to_string()),
            end_at: Some("2026-07-31T23:59:59Z".to_string()),
        }),
        first_seen_at: "2026-07-10T14:05:00Z".to_string(),
        last_seen_at: "2026-07-10T14:05:00Z".to_string(),
        occurrence_summary: canon::inbox::OccurrenceSummary::default(),
        occurrences: vec![InboxOccurrenceRef {
            project_ref: "project.alpha".to_string(),
            run_ref: "run-001".to_string(),
            source_ref: "fixtures/sec10d/issuer.csv".to_string(),
            record_ref: Some("row-7".to_string()),
            seen_at: "2026-07-10T14:05:00Z".to_string(),
        }],
        privacy_class: Some(PrivacyClass::Internal),
        raw_values_redacted: false,
        raw_values: vec![ExternalRawValueReference {
            store: "vault://canon".to_string(),
            locator: "raw/issuer_name/project.alpha/run-001/row-7".to_string(),
            content_hash: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        }],
    }
}

fn sample_fingerprint(surface_role: &str, fingerprint: &str) -> NormalizedSurfaceFingerprint {
    NormalizedSurfaceFingerprint {
        normalizer_id: "namekit.v0/ascii_trim_lower".to_string(),
        surface_role: surface_role.to_string(),
        fingerprint: fingerprint.to_string(),
    }
}
