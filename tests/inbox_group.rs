#![forbid(unsafe_code)]

mod inbox {
    pub use canon::inbox::*;
}

#[path = "../src/inbox/group.rs"]
mod inbox_group;

use canon::inbox::{
    CandidateStatus, InboxEventKind, InboxExportMode, InboxFieldRole, InboxMergeMode,
    InboxOccurrenceRef, InboxPrivacyPolicy, InboxReasonCode, NamespaceHint,
    NormalizedSurfaceFingerprint, OccurrenceSummary, ProfileFieldRef, RawValueRetention,
    TemporalScope, UnresolvedInboxArtifact, UnresolvedInboxItem, finalize_artifact,
};
use inbox_group::{
    CANON_UNRESOLVED_GROUPS_VERSION, CannotGroupRule, GROUP_IDENTITY_STATUS, GroupReviewAction,
    GroupReviewPatch, UnresolvedGroupingPlan, canonical_group_json_bytes,
    group_unresolved_artifact,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.unresolved.groups.v1.schema.json");

#[test]
fn schema_declares_triage_not_identity_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_UNRESOLVED_GROUPS_VERSION);
    assert_eq!(
        schema["properties"]["identity_status"]["const"],
        GROUP_IDENTITY_STATUS
    );
    assert_eq!(
        schema["$defs"]["blake3"]["pattern"],
        "^blake3:[0-9a-f]{64}$"
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("no canonical identity assertion")
    );
}

#[test]
fn repeated_and_reordered_events_yield_identical_groups_and_representatives() {
    let later = sample_item(ItemSpec {
        record_ref: "row-2",
        seen_at: "2026-07-10T15:00:00Z",
        primary: '1',
        variant: 'a',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let earlier = sample_item(ItemSpec {
        record_ref: "row-1",
        seen_at: "2026-07-10T14:00:00Z",
        primary: '1',
        variant: 'b',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });

    let first = finalized_inbox(vec![later.clone(), earlier.clone()]);
    let second = finalized_inbox(vec![earlier, later]);
    let grouped_first =
        group_unresolved_artifact(&first, grouping_plan()).expect("first grouping succeeds");
    let grouped_second =
        group_unresolved_artifact(&second, grouping_plan()).expect("second grouping succeeds");

    assert_eq!(
        canonical_group_json_bytes(&grouped_first).unwrap(),
        canonical_group_json_bytes(&grouped_second).unwrap()
    );
    assert_eq!(grouped_first.identity_status, GROUP_IDENTITY_STATUS);
    assert_eq!(grouped_first.summary.total_groups, 1);
    assert_eq!(grouped_first.summary.total_members, 2);
    assert_eq!(grouped_first.groups[0].member_count, 2);
    assert_eq!(
        grouped_first.groups[0].occurrence_summary.distinct_sources,
        2
    );
    assert_eq!(
        grouped_first.groups[0].representative_event_key,
        first.items[0].event_key
    );
    assert_eq!(grouped_first.groups[0].members[0].occurrences.len(), 1);
    assert!(grouped_first.groups[0].review_patch_ids.is_empty());
}

#[test]
fn protected_tokens_namespaces_temporal_scopes_and_cannot_rules_split_groups() {
    let base = sample_item(ItemSpec {
        record_ref: "base",
        seen_at: "2026-07-10T14:00:00Z",
        primary: '2',
        variant: 'a',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let otherwise_same = sample_item(ItemSpec {
        record_ref: "cannot",
        seen_at: "2026-07-10T14:01:00Z",
        primary: '2',
        variant: 'b',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let protected_a = sample_item(ItemSpec {
        record_ref: "protected-a",
        seen_at: "2026-07-10T14:02:00Z",
        primary: '2',
        variant: 'c',
        protected: Some('a'),
        namespace: "issuer",
        temporal: None,
    });
    let protected_b = sample_item(ItemSpec {
        record_ref: "protected-b",
        seen_at: "2026-07-10T14:03:00Z",
        primary: '2',
        variant: 'd',
        protected: Some('b'),
        namespace: "issuer",
        temporal: None,
    });
    let other_namespace = sample_item(ItemSpec {
        record_ref: "namespace",
        seen_at: "2026-07-10T14:04:00Z",
        primary: '2',
        variant: 'e',
        protected: None,
        namespace: "borrower",
        temporal: None,
    });
    let temporal = sample_item(ItemSpec {
        record_ref: "temporal",
        seen_at: "2026-07-10T14:05:00Z",
        primary: '2',
        variant: 'f',
        protected: None,
        namespace: "issuer",
        temporal: Some(("2026-01-01T00:00:00Z", "2026-01-31T00:00:00Z")),
    });
    let inbox = finalized_inbox(vec![
        base,
        otherwise_same,
        protected_a,
        protected_b,
        other_namespace,
        temporal,
    ]);
    let mut plan = grouping_plan();
    plan.cannot_group = vec![CannotGroupRule {
        rule_id: "cannot-001".to_string(),
        left_event_key: inbox.items[0].event_key.clone(),
        right_event_key: inbox.items[1].event_key.clone(),
        reason: "operator-marked-distinct-surfaces".to_string(),
    }];

    let grouped = group_unresolved_artifact(&inbox, plan).expect("grouping succeeds");

    assert_eq!(grouped.summary.total_groups, 6);
    assert_eq!(grouped.summary.total_members, 6);
    assert!(grouped.groups.iter().all(|group| group.member_count == 1));
}

#[test]
fn reviewed_split_and_merge_patches_are_provenance_bearing_and_reversible() {
    let first = sample_item(ItemSpec {
        record_ref: "first",
        seen_at: "2026-07-10T14:00:00Z",
        primary: '3',
        variant: 'a',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let second = sample_item(ItemSpec {
        record_ref: "second",
        seen_at: "2026-07-10T14:01:00Z",
        primary: '3',
        variant: 'b',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let split_inbox = finalized_inbox(vec![first, second]);
    let split_key = split_inbox.items[1].event_key.clone();
    let mut split_plan = grouping_plan();
    split_plan.review_patches = vec![review_patch(
        "patch-split-001",
        GroupReviewAction::Split,
        vec![split_key.clone()],
    )];

    let split = group_unresolved_artifact(&split_inbox, split_plan).expect("split succeeds");
    assert_eq!(split.summary.total_groups, 2);
    let split_group = group_containing(&split, &split_key);
    assert_eq!(split_group.review_patch_ids, vec!["patch-split-001"]);
    assert_eq!(
        split_group.members[0].occurrences[0].record_ref.as_deref(),
        Some("second")
    );

    let merge_left = sample_item(ItemSpec {
        record_ref: "merge-left",
        seen_at: "2026-07-10T14:00:00Z",
        primary: '4',
        variant: 'a',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let merge_right = sample_item(ItemSpec {
        record_ref: "merge-right",
        seen_at: "2026-07-10T14:01:00Z",
        primary: '5',
        variant: 'b',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let merge_inbox = finalized_inbox(vec![merge_left, merge_right]);
    let mut merge_plan = grouping_plan();
    merge_plan.review_patches = vec![review_patch(
        "patch-merge-001",
        GroupReviewAction::Merge,
        merge_inbox
            .items
            .iter()
            .map(|item| item.event_key.clone())
            .collect(),
    )];

    let merged = group_unresolved_artifact(&merge_inbox, merge_plan).expect("merge succeeds");
    assert_eq!(merged.summary.total_groups, 1);
    assert_eq!(merged.groups[0].member_count, 2);
    assert_eq!(merged.groups[0].grouping_keys.len(), 2);
    assert_eq!(merged.groups[0].review_patch_ids, vec!["patch-merge-001"]);
    assert_eq!(merged.groups[0].occurrence_summary.total_occurrences, 2);
}

#[test]
fn reviewed_merge_patch_refuses_hard_boundary_conflict() {
    let issuer = sample_item(ItemSpec {
        record_ref: "issuer",
        seen_at: "2026-07-10T14:00:00Z",
        primary: '6',
        variant: 'a',
        protected: None,
        namespace: "issuer",
        temporal: None,
    });
    let borrower = sample_item(ItemSpec {
        record_ref: "borrower",
        seen_at: "2026-07-10T14:01:00Z",
        primary: '7',
        variant: 'b',
        protected: None,
        namespace: "borrower",
        temporal: None,
    });
    let inbox = finalized_inbox(vec![issuer, borrower]);
    let mut plan = grouping_plan();
    plan.review_patches = vec![review_patch(
        "patch-merge-conflict",
        GroupReviewAction::Merge,
        inbox
            .items
            .iter()
            .map(|item| item.event_key.clone())
            .collect(),
    )];

    let error = group_unresolved_artifact(&inbox, plan).expect_err("merge should refuse");
    assert_eq!(error.code, canon::inbox::InboxErrorCode::ArtifactContract);
    assert!(
        error
            .message
            .contains("protected/namespace/temporal boundary")
    );
}

fn grouping_plan() -> UnresolvedGroupingPlan {
    UnresolvedGroupingPlan {
        policy_id: "policy.groups.v1".to_string(),
        grouping_surface_roles: vec!["primary".to_string()],
        protected_surface_roles: vec!["protected_token".to_string()],
        cannot_group: Vec::new(),
        review_patches: Vec::new(),
    }
}

fn review_patch(
    patch_id: &str,
    action: GroupReviewAction,
    member_event_keys: Vec<String>,
) -> GroupReviewPatch {
    GroupReviewPatch {
        patch_id: patch_id.to_string(),
        action,
        member_event_keys,
        operator_ref: "operator.zac".to_string(),
        reason: "reviewed triage grouping correction".to_string(),
        reviewed_at: "2026-07-10T12:00:00-04:00".to_string(),
    }
}

fn group_containing<'a>(
    artifact: &'a inbox_group::UnresolvedGroupsArtifact,
    event_key: &str,
) -> &'a inbox_group::UnresolvedGroup {
    artifact
        .groups
        .iter()
        .find(|group| {
            group
                .members
                .iter()
                .any(|member| member.event_key == event_key)
        })
        .expect("group exists")
}

fn finalized_inbox(items: Vec<UnresolvedInboxItem>) -> UnresolvedInboxArtifact {
    finalize_artifact(UnresolvedInboxArtifact {
        version: canon::inbox::CANON_UNRESOLVED_INBOX_VERSION.to_string(),
        view: InboxExportMode::Redacted,
        artifact_content_hash: String::new(),
        policy: InboxPrivacyPolicy {
            policy_id: "policy.default".to_string(),
            raw_value_retention: RawValueRetention::Omit,
            default_export_mode: InboxExportMode::Redacted,
            merge_mode: InboxMergeMode::Strict,
        },
        summary: canon::inbox::InboxSummary::default(),
        items,
    })
    .expect("inbox finalizes")
}

#[derive(Clone, Copy)]
struct ItemSpec<'a> {
    record_ref: &'a str,
    seen_at: &'a str,
    primary: char,
    variant: char,
    protected: Option<char>,
    namespace: &'a str,
    temporal: Option<(&'a str, &'a str)>,
}

fn sample_item(spec: ItemSpec<'_>) -> UnresolvedInboxItem {
    let mut fingerprints = vec![
        fingerprint("primary", spec.primary),
        fingerprint("format_variant", spec.variant),
    ];
    if let Some(protected) = spec.protected {
        fingerprints.push(fingerprint("protected_token", protected));
    }

    UnresolvedInboxItem {
        event_key: String::new(),
        event_kind: InboxEventKind::ExactLookup,
        reason_code: InboxReasonCode::NoMatchingRule,
        field_name: "issuer_name".to_string(),
        field_role: InboxFieldRole::NameField,
        profile_ref: Some(ProfileFieldRef {
            profile_id: "sec10d_issuer".to_string(),
            profile_version: "1.0.0".to_string(),
        }),
        surface_fingerprints: fingerprints,
        namespace_hints: vec![NamespaceHint {
            namespace: spec.namespace.to_string(),
            source: "profile".to_string(),
        }],
        candidate_summary: canon::inbox::CandidateSummary {
            status: CandidateStatus::None,
            candidate_count: 0,
            best_score_band: None,
            rejection_reasons: Vec::new(),
        },
        temporal_scope: spec.temporal.map(|(start_at, end_at)| TemporalScope {
            start_at: Some(start_at.to_string()),
            end_at: Some(end_at.to_string()),
        }),
        first_seen_at: String::new(),
        last_seen_at: String::new(),
        occurrence_summary: OccurrenceSummary::default(),
        occurrences: vec![InboxOccurrenceRef {
            project_ref: "project.alpha".to_string(),
            run_ref: "run-001".to_string(),
            source_ref: format!("source/{}.csv", spec.record_ref),
            record_ref: Some(spec.record_ref.to_string()),
            seen_at: spec.seen_at.to_string(),
        }],
        privacy_class: None,
        raw_values_redacted: false,
        raw_values: Vec::new(),
    }
}

fn fingerprint(surface_role: &str, hex: char) -> NormalizedSurfaceFingerprint {
    NormalizedSurfaceFingerprint {
        normalizer_id: "namekit.v0/test".to_string(),
        surface_role: surface_role.to_string(),
        fingerprint: digest(hex),
    }
}

fn digest(hex: char) -> String {
    assert!(hex.is_ascii_digit() || ('a'..='f').contains(&hex));
    format!("blake3:{}", hex.to_string().repeat(64))
}
