#![forbid(unsafe_code)]

use canon::inbox::rank::{
    self as inbox_rank, CANON_INBOX_PRIORITY_POLICY_VERSION, PriorityPolicy,
    PrioritySignalOverride, RANKING_IDENTITY_STATUS, canonical_priority_json_bytes, rank_inbox,
};
use canon::inbox::{
    self, CandidateStatus, InboxEventKind, InboxExportMode, InboxFieldRole, InboxMergeMode,
    InboxOccurrenceRef, InboxPrivacyPolicy, InboxReasonCode, NamespaceHint,
    NormalizedSurfaceFingerprint, OccurrenceSummary, PrivacyClass, ProfileFieldRef,
    RawValueRetention, UnresolvedInboxArtifact, UnresolvedInboxItem, finalize_artifact,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.inbox.priority_policy.v1.schema.json");

#[test]
fn schema_declares_expected_coverage_policy_not_identity_or_financial_decision() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_INBOX_PRIORITY_POLICY_VERSION);
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("expected coverage value")
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("makes no canonical identity assertion")
    );
    assert!(!schema.to_string().to_lowercase().contains("roi"));
    assert_eq!(
        schema["properties"]["component_weights"]["required"][6],
        "downstream_consumers"
    );
    assert!(
        schema["properties"]["event_signals"]["description"]
            .as_str()
            .unwrap()
            .contains("without rewriting source events")
    );
}

#[test]
fn shuffled_inputs_rank_identically_with_stable_tie_breaks() {
    let older = sample_item(ItemSpec {
        label: "older",
        occurrences: 2,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::NameField,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 1,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "people",
    });
    let newer = sample_item(ItemSpec {
        label: "newer",
        occurrences: 2,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::NameField,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 4,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "people",
    });
    let first = finalized_inbox(vec![newer.clone(), older.clone()]);
    let second = finalized_inbox(vec![older, newer]);
    let policy = policy_with_signals(&first, |_| PrioritySignalOverride {
        distinct_subjects: Some(1),
        downstream_consumers: Some(1),
        registry_id: Some("people".to_string()),
        source_partition: Some("source.partition".to_string()),
        ..PrioritySignalOverride::default()
    });

    let ranked_first = rank_inbox(&first, policy.clone()).expect("first ranking succeeds");
    let ranked_second = rank_inbox(&second, policy).expect("second ranking succeeds");

    assert_eq!(
        canonical_priority_json_bytes(&ranked_first).unwrap(),
        canonical_priority_json_bytes(&ranked_second).unwrap()
    );
    assert_eq!(
        ranked_first.ranked_items[0].tie_breaker[0],
        "2026-07-10T01:00:00Z"
    );
    assert_eq!(ranked_first.ranked_items[0].rank, 1);
    assert_eq!(ranked_first.ranked_items[1].rank, 2);
}

#[test]
fn high_impact_ambiguous_item_can_outrank_easy_low_value_alias_without_decision() {
    let easy = sample_item(ItemSpec {
        label: "easy",
        occurrences: 1,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::ContextField,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 3,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Public),
        namespace: "registry.low",
    });
    let ambiguous = sample_item(ItemSpec {
        label: "ambiguous",
        occurrences: 9,
        projects: 4,
        sources: 3,
        role: InboxFieldRole::NameField,
        candidate_status: CandidateStatus::Ambiguous,
        candidate_count: 6,
        first_hour: 2,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Restricted),
        namespace: "registry.high",
    });
    let inbox = finalized_inbox(vec![easy, ambiguous]);
    let policy = policy_with_signals(&inbox, |item| {
        if item.field_role == InboxFieldRole::NameField {
            PrioritySignalOverride {
                distinct_subjects: Some(3),
                downstream_consumers: Some(7),
                exposure_band: Some("high".to_string()),
                registry_id: Some("registry.high".to_string()),
                source_partition: Some("critical.sources".to_string()),
                ..PrioritySignalOverride::default()
            }
        } else {
            PrioritySignalOverride {
                distinct_subjects: Some(1),
                downstream_consumers: Some(0),
                exposure_band: Some("low".to_string()),
                registry_id: Some("registry.low".to_string()),
                source_partition: Some("quiet.sources".to_string()),
                ..PrioritySignalOverride::default()
            }
        }
    });

    let ranked = rank_inbox(&inbox, policy).expect("ranking succeeds");
    let top = &ranked.ranked_items[0];

    assert_eq!(top.identity_status, RANKING_IDENTITY_STATUS);
    assert_eq!(top.queue_partition.registry, "registry.high");
    assert_component_positive(top, "candidate_readiness");
    assert_component_positive(top, "ambiguity_cost");
    assert!(
        top.expected_coverage_value_units > ranked.ranked_items[1].expected_coverage_value_units
    );
}

#[test]
fn missing_signals_and_capped_outliers_have_explicit_contributions() {
    let outlier = sample_item(ItemSpec {
        label: "outlier",
        occurrences: 150,
        projects: 30,
        sources: 30,
        role: InboxFieldRole::LookupInput,
        candidate_status: CandidateStatus::Ambiguous,
        candidate_count: 120,
        first_hour: 1,
        profile: None,
        privacy: None,
        namespace: "registry.outlier",
    });
    let inbox = finalized_inbox(vec![outlier]);
    let mut policy = PriorityPolicy::baseline("policy.priority", "rev-a", "2026-07-11T00:00:00Z");
    policy.caps.insert("recurrence".to_string(), 10);
    policy.caps.insert("distinct_sources".to_string(), 5);
    policy.caps.insert("distinct_projects".to_string(), 5);
    policy.caps.insert("ambiguity_cost".to_string(), 8);

    let ranked = rank_inbox(&inbox, policy).expect("ranking succeeds");
    let item = &ranked.ranked_items[0];

    assert!(
        item.uncertainty_flags
            .contains(&"missing_profile".to_string())
    );
    assert!(
        item.uncertainty_flags
            .contains(&"missing_privacy_class".to_string())
    );
    assert!(
        item.uncertainty_flags
            .contains(&"missing_distinct_subjects".to_string())
    );
    assert!(
        item.uncertainty_flags
            .contains(&"missing_downstream_consumers".to_string())
    );
    assert!(
        item.uncertainty_flags
            .contains(&"capped_recurrence".to_string())
    );
    assert_eq!(
        component(item, "recurrence").effective_value,
        10,
        "recurrence contribution uses the cap"
    );
    assert_eq!(
        component(item, "downstream_consumers").contribution_units,
        0,
        "missing downstream context contributes explicitly as zero"
    );
    assert!(ranked.summary.capped_outlier_items > 0);
    assert!(ranked.summary.uncertain_items > 0);
}

#[test]
fn policy_revision_reranks_without_rewriting_source_events() {
    let high_recurrence = sample_item(ItemSpec {
        label: "name",
        occurrences: 10,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::NameField,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 1,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "registry.shared",
    });
    let anchor = sample_item(ItemSpec {
        label: "anchor",
        occurrences: 1,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::AnchorField,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 2,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "registry.shared",
    });
    let inbox = finalized_inbox(vec![high_recurrence, anchor]);
    let base = policy_with_signals(&inbox, |_| PrioritySignalOverride {
        distinct_subjects: Some(1),
        downstream_consumers: Some(0),
        registry_id: Some("registry.shared".to_string()),
        ..PrioritySignalOverride::default()
    });
    let mut role_heavy = base.clone();
    role_heavy.revision = "rev-role-heavy".to_string();
    role_heavy
        .component_weights
        .insert("recurrence".to_string(), 1);
    role_heavy
        .component_weights
        .insert("role_criticality".to_string(), 200);

    let base_ranked = rank_inbox(&inbox, base).expect("base ranking succeeds");
    let role_ranked = rank_inbox(&inbox, role_heavy).expect("role-heavy ranking succeeds");

    assert_eq!(
        base_ranked.source_inbox_artifact_hash,
        role_ranked.source_inbox_artifact_hash
    );
    assert_ne!(
        base_ranked.policy_content_hash,
        role_ranked.policy_content_hash
    );
    assert_ne!(
        base_ranked.ranked_items[0].event_key, role_ranked.ranked_items[0].event_key,
        "policy revision changes the explanation and order without changing events"
    );
    assert_component_positive(&role_ranked.ranked_items[0], "role_criticality");
}

#[test]
fn recurrence_component_is_monotonic() {
    let one = finalized_inbox(vec![sample_item(ItemSpec {
        label: "same",
        occurrences: 1,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::LookupInput,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 1,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "registry.shared",
    })]);
    let three = finalized_inbox(vec![sample_item(ItemSpec {
        label: "same",
        occurrences: 3,
        projects: 1,
        sources: 1,
        role: InboxFieldRole::LookupInput,
        candidate_status: CandidateStatus::None,
        candidate_count: 0,
        first_hour: 1,
        profile: Some("profile.alpha"),
        privacy: Some(PrivacyClass::Internal),
        namespace: "registry.shared",
    })]);
    let policy = PriorityPolicy::baseline("policy.priority", "rev-a", "2026-07-11T00:00:00Z");

    let one_ranked = rank_inbox(&one, policy.clone()).expect("one occurrence ranks");
    let three_ranked = rank_inbox(&three, policy).expect("three occurrences rank");

    assert!(
        component(&three_ranked.ranked_items[0], "recurrence").contribution_units
            > component(&one_ranked.ranked_items[0], "recurrence").contribution_units
    );
}

fn component<'a>(
    item: &'a inbox_rank::RankedInboxItem,
    name: &str,
) -> &'a inbox_rank::PriorityComponentScore {
    item.components
        .iter()
        .find(|component| component.component == name)
        .unwrap_or_else(|| panic!("missing component {name}"))
}

fn assert_component_positive(item: &inbox_rank::RankedInboxItem, name: &str) {
    assert!(
        component(item, name).contribution_units > 0,
        "{name} should contribute positively: {:?}",
        component(item, name)
    );
}

fn policy_with_signals(
    inbox: &UnresolvedInboxArtifact,
    signal_for: impl Fn(&UnresolvedInboxItem) -> PrioritySignalOverride,
) -> PriorityPolicy {
    let mut policy = PriorityPolicy::baseline("policy.priority", "rev-a", "2026-07-11T00:00:00Z");
    policy.event_signals = inbox
        .items
        .iter()
        .map(|item| (item.event_key.clone(), signal_for(item)))
        .collect();
    policy
}

fn finalized_inbox(items: Vec<UnresolvedInboxItem>) -> UnresolvedInboxArtifact {
    finalize_artifact(UnresolvedInboxArtifact {
        version: inbox::CANON_UNRESOLVED_INBOX_VERSION.to_string(),
        view: InboxExportMode::Redacted,
        artifact_content_hash: String::new(),
        policy: InboxPrivacyPolicy {
            policy_id: "policy.default".to_string(),
            raw_value_retention: RawValueRetention::Omit,
            default_export_mode: InboxExportMode::Redacted,
            merge_mode: InboxMergeMode::Strict,
        },
        summary: inbox::InboxSummary::default(),
        items,
    })
    .expect("inbox finalizes")
}

#[derive(Clone, Copy)]
struct ItemSpec<'a> {
    label: &'a str,
    occurrences: usize,
    projects: usize,
    sources: usize,
    role: InboxFieldRole,
    candidate_status: CandidateStatus,
    candidate_count: u32,
    first_hour: u32,
    profile: Option<&'a str>,
    privacy: Option<PrivacyClass>,
    namespace: &'a str,
}

fn sample_item(spec: ItemSpec<'_>) -> UnresolvedInboxItem {
    UnresolvedInboxItem {
        event_key: String::new(),
        event_kind: InboxEventKind::ExactLookup,
        reason_code: match spec.candidate_status {
            CandidateStatus::Ambiguous => InboxReasonCode::AmbiguousCandidates,
            CandidateStatus::Rejected => InboxReasonCode::CannotLink,
            CandidateStatus::BudgetLimited => InboxReasonCode::BudgetExceeded,
            CandidateStatus::None => InboxReasonCode::NoMatchingRule,
        },
        field_name: "issuer_name".to_string(),
        field_role: spec.role,
        profile_ref: spec.profile.map(|profile_id| ProfileFieldRef {
            profile_id: profile_id.to_string(),
            profile_version: "1.0.0".to_string(),
        }),
        surface_fingerprints: vec![fingerprint("primary", digest_char(spec.label, 0))],
        namespace_hints: vec![NamespaceHint {
            namespace: spec.namespace.to_string(),
            source: "profile".to_string(),
        }],
        candidate_summary: inbox::CandidateSummary {
            status: spec.candidate_status,
            candidate_count: spec.candidate_count,
            best_score_band: None,
            rejection_reasons: Vec::new(),
        },
        temporal_scope: None,
        first_seen_at: String::new(),
        last_seen_at: String::new(),
        occurrence_summary: OccurrenceSummary::default(),
        occurrences: occurrences(&spec),
        privacy_class: spec.privacy,
        raw_values_redacted: false,
        raw_values: Vec::new(),
    }
}

fn occurrences(spec: &ItemSpec<'_>) -> Vec<InboxOccurrenceRef> {
    (0..spec.occurrences)
        .map(|index| InboxOccurrenceRef {
            project_ref: format!("project-{}", index % spec.projects.max(1)),
            run_ref: format!("run-{}", index),
            source_ref: format!("source-{}", index % spec.sources.max(1)),
            record_ref: Some(format!("{}-{index}", spec.label)),
            seen_at: format!(
                "2026-07-10T{:02}:00:00Z",
                spec.first_hour + (index as u32 % 20)
            ),
        })
        .collect()
}

fn fingerprint(surface_role: &str, hex: char) -> NormalizedSurfaceFingerprint {
    NormalizedSurfaceFingerprint {
        normalizer_id: "namekit.v0/test".to_string(),
        surface_role: surface_role.to_string(),
        fingerprint: digest(hex),
    }
}

fn digest_char(label: &str, offset: usize) -> char {
    let value = label
        .bytes()
        .fold(offset as u8, |acc, byte| acc.wrapping_add(byte))
        % 16;
    b"0123456789abcdef"[value as usize] as char
}

fn digest(hex: char) -> String {
    assert!(hex.is_ascii_digit() || ('a'..='f').contains(&hex));
    format!("blake3:{}", hex.to_string().repeat(64))
}
