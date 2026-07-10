#![forbid(unsafe_code)]

mod temporal_impl {
    pub mod fact {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/temporal/fact.rs"));
    }
    pub mod alias {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/alias.rs"
        ));
    }
    pub mod conflict {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/conflict.rs"
        ));
    }
}

use temporal_impl::alias::{
    AliasClaim, AliasScope, AliasValueKind, AnchorExclusivity, LookupVisibility,
    PromotionProvenance, TrustedAnchor, compile_alias_snapshot, finalize_alias_claim,
    global_exact_lookup_claims, source_exact_lookup_claims,
};
use temporal_impl::conflict::{
    CANON_TEMPORAL_CONFLICT_POLICY_VERSION, ConflictArtifact, ConflictClass, ConflictDisposition,
    ConflictPolicy, ConflictPolicyClause, ConflictResolution, compile_conflict_artifact,
};
use temporal_impl::fact::{
    AssertionStatus, CANON_IDENTITY_FACT_VERSION, FactScope, IdentityFact, IntervalBoundary,
    RecordedTime, SourceLocator, TimeInterval,
};

#[test]
fn source_scoped_alias_requires_explicit_promotion_for_global_lookup() {
    let local = sample_claim("ACME LOCAL", "ent:alpha", "feed_a");
    let snapshot = compile_alias_snapshot(
        std::slice::from_ref(&local),
        "2026-07-15T00:00:00Z",
        "2026-07-20T00:00:00Z",
    )
    .expect("local snapshot compiles");
    assert!(global_exact_lookup_claims(&snapshot).is_empty());
    assert_eq!(
        source_exact_lookup_claims(&snapshot, "feed_a", Some("product_line"), Some("book_a"))
            .expect("source lookup compiles")
            .len(),
        1
    );

    let promoted = AliasClaim {
        lookup_visibility: LookupVisibility::Global,
        promoted_to_global_by: Some(PromotionProvenance {
            policy_clause_id: "promote-local-alias".to_string(),
            evidence_ref: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .to_string(),
        }),
        ..local
    };
    let promoted_snapshot =
        compile_alias_snapshot(&[promoted], "2026-07-15T00:00:00Z", "2026-07-20T00:00:00Z")
            .expect("promoted snapshot compiles");
    assert_eq!(global_exact_lookup_claims(&promoted_snapshot).len(), 1);
}

#[test]
fn identity_fact_contract_carries_scope_and_temporal_metadata() {
    let fact = IdentityFact {
        version: CANON_IDENTITY_FACT_VERSION.to_string(),
        fact_id: "fact:alias:1".to_string(),
        assertion_key: "alias:acme".to_string(),
        conflict_key: "name:acme".to_string(),
        subject_id: "ent:alpha".to_string(),
        predicate: "known_as".to_string(),
        object_id: "alias:ACME".to_string(),
        valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time("2026-01-02T00:00:00Z", 7),
        source_locator: SourceLocator {
            source_system: "feed_a".to_string(),
            locator: "fixtures/feed_a.jsonl".to_string(),
            fragment: Some("row-7".to_string()),
        },
        materialization_digest: sample_hash('f'),
        assertion_status: AssertionStatus::Accepted,
        trust_policy_ref: "trust.default.v1".to_string(),
        scope: Some(FactScope {
            scope_type: "product_line".to_string(),
            scope_id: "book_a".to_string(),
        }),
        supersedes: Vec::new(),
        retracts: Vec::new(),
    };

    assert_eq!(fact.version, CANON_IDENTITY_FACT_VERSION);
    assert_eq!(fact.scope.as_ref().expect("fact scope").scope_id, "book_a");
    assert_eq!(fact.recorded_time.transaction_seq, Some(7));
}

#[test]
fn late_correction_changes_known_as_of_snapshot_but_history_remains_queryable() {
    let original = finalize_alias_claim(AliasClaim {
        recorded_time: recorded_time("2026-01-05T00:00:00Z", 1),
        ..global_claim("ACME HOLDINGS", "ent:alpha", "source_a")
    })
    .expect("original claim finalizes");
    let correction = AliasClaim {
        entity_id: "ent:beta".to_string(),
        recorded_time: recorded_time("2026-02-01T00:00:00Z", 2),
        supersedes: vec![original.claim_id.clone()],
        ..global_claim("ACME HOLDINGS", "ent:beta", "source_a")
    };

    let before = compile_alias_snapshot(
        &[original.clone(), correction.clone()],
        "2026-03-01T00:00:00Z",
        "2026-01-10T00:00:00Z",
    )
    .expect("pre-correction snapshot compiles");
    assert_eq!(before.active_claims.len(), 1);
    assert_eq!(before.active_claims[0].entity_id, "ent:alpha");

    let after = compile_alias_snapshot(
        &[original.clone(), correction.clone()],
        "2026-03-01T00:00:00Z",
        "2026-02-15T00:00:00Z",
    )
    .expect("post-correction snapshot compiles");
    assert_eq!(after.active_claims.len(), 1);
    assert_eq!(after.active_claims[0].entity_id, "ent:beta");
    assert!(after.suppressed_claim_ids.contains(&original.claim_id));
    assert_eq!(after.history.len(), 2);

    let artifact = compile_conflict_artifact(
        &[original, correction],
        empty_policy("history-only"),
        "2026-03-01T00:00:00Z",
        "2026-02-15T00:00:00Z",
    )
    .expect("conflict artifact compiles");
    assert_eq!(
        find_conflict(&artifact, ConflictClass::Correction)
            .expect("correction conflict")
            .disposition,
        ConflictDisposition::HistoricalOnly
    );
}

#[test]
fn overlapping_exclusive_anchors_abstain_without_policy_and_resolve_with_named_policy() {
    let left = AliasClaim {
        trusted_anchor: Some(TrustedAnchor {
            namespace: "lei".to_string(),
            value: "LEI-123".to_string(),
            exclusivity: AnchorExclusivity::Exclusive,
        }),
        ..global_claim("ACME", "ent:alpha", "source_a")
    };
    let right = AliasClaim {
        trusted_anchor: Some(TrustedAnchor {
            namespace: "lei".to_string(),
            value: "LEI-123".to_string(),
            exclusivity: AnchorExclusivity::Exclusive,
        }),
        recorded_time: recorded_time("2026-01-03T00:00:00Z", 2),
        ..global_claim("ACME CORP", "ent:beta", "source_b")
    };

    let abstaining = compile_conflict_artifact(
        &[left.clone(), right.clone()],
        empty_policy("abstain"),
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
    )
    .expect("abstaining artifact compiles");
    let abstaining_conflict = find_conflict(&abstaining, ConflictClass::OverlappingExclusiveAnchor)
        .expect("anchor conflict");
    assert_eq!(
        abstaining_conflict.disposition,
        ConflictDisposition::Abstain
    );
    assert!(abstaining.global_exact_claim_ids.is_empty());

    let policy = source_precedence_policy(
        "anchor-precedence",
        ConflictClass::OverlappingExclusiveAnchor,
        &["source_b", "source_a"],
    );
    let resolved = compile_conflict_artifact(
        &[left, right],
        policy,
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
    )
    .expect("resolved artifact compiles");
    let resolved_conflict = find_conflict(&resolved, ConflictClass::OverlappingExclusiveAnchor)
        .expect("anchor conflict");
    assert_eq!(resolved_conflict.disposition, ConflictDisposition::Resolved);
    assert_eq!(resolved.global_exact_claim_ids.len(), 1);
    assert_eq!(
        resolved.global_exact_claim_ids[0],
        resolved_conflict
            .winning_claim_id
            .clone()
            .expect("winning claim id")
    );
}

#[test]
fn simultaneous_source_disagreement_policy_revisions_are_deterministic() {
    let left = global_claim("ACME", "ent:alpha", "source_a");
    let right = AliasClaim {
        recorded_time: recorded_time("2026-01-04T00:00:00Z", 4),
        ..global_claim("ACME", "ent:beta", "source_b")
    };

    let policy_a = source_precedence_policy(
        "prefer-a",
        ConflictClass::SourceDisagreement,
        &["source_a", "source_b"],
    );
    let artifact_a = compile_conflict_artifact(
        &[left.clone(), right.clone()],
        policy_a,
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
    )
    .expect("artifact a compiles");

    let policy_b = source_precedence_policy(
        "prefer-b",
        ConflictClass::SourceDisagreement,
        &["source_b", "source_a"],
    );
    let artifact_b = compile_conflict_artifact(
        &[left, right],
        policy_b,
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
    )
    .expect("artifact b compiles");

    let conflict_a =
        find_conflict(&artifact_a, ConflictClass::SourceDisagreement).expect("conflict a");
    let conflict_b =
        find_conflict(&artifact_b, ConflictClass::SourceDisagreement).expect("conflict b");
    assert_eq!(conflict_a.disposition, ConflictDisposition::Resolved);
    assert_eq!(conflict_b.disposition, ConflictDisposition::Resolved);
    assert_ne!(
        artifact_a.global_exact_claim_ids,
        artifact_b.global_exact_claim_ids
    );
    assert_ne!(
        conflict_a.policy_clause_ids_used,
        conflict_b.policy_clause_ids_used
    );
}

#[test]
fn retractions_clear_multiple_entity_conflict_for_later_snapshot() {
    let accepted_left = finalize_alias_claim(global_claim("ACME REUSED", "ent:alpha", "source_a"))
        .expect("left claim finalizes");
    let accepted_right = finalize_alias_claim(AliasClaim {
        recorded_time: recorded_time("2026-01-02T00:00:00Z", 2),
        ..global_claim("ACME REUSED", "ent:beta", "source_a")
    })
    .expect("right claim finalizes");
    let retraction = AliasClaim {
        assertion_status: AssertionStatus::Retracted,
        recorded_time: recorded_time("2026-02-01T00:00:00Z", 3),
        retracts: vec![accepted_right.claim_id.clone()],
        ..global_claim("ACME REUSED", "ent:beta", "source_a")
    };

    let before = compile_conflict_artifact(
        &[
            accepted_left.clone(),
            accepted_right.clone(),
            retraction.clone(),
        ],
        empty_policy("before"),
        "2026-03-01T00:00:00Z",
        "2026-01-15T00:00:00Z",
    )
    .expect("pre-retraction artifact compiles");
    assert_eq!(
        find_conflict(&before, ConflictClass::AliasToMultipleEntityClaim)
            .expect("pre-retraction conflict")
            .disposition,
        ConflictDisposition::Abstain
    );

    let after = compile_conflict_artifact(
        &[accepted_left.clone(), accepted_right.clone(), retraction],
        empty_policy("after"),
        "2026-03-01T00:00:00Z",
        "2026-02-15T00:00:00Z",
    )
    .expect("post-retraction artifact compiles");
    assert!(find_conflict(&after, ConflictClass::AliasToMultipleEntityClaim).is_none());
    assert_eq!(
        find_conflict(&after, ConflictClass::Retraction)
            .expect("retraction event")
            .disposition,
        ConflictDisposition::HistoricalOnly
    );
    assert_eq!(after.global_exact_claim_ids.len(), 1);
    assert_eq!(after.active_claim_ids, vec![accepted_left.claim_id]);
}

#[test]
fn recycled_identifier_and_interval_gap_are_history_safe() {
    let first = AliasClaim {
        trusted_anchor: Some(TrustedAnchor {
            namespace: "loan_id".to_string(),
            value: "L-100".to_string(),
            exclusivity: AnchorExclusivity::Exclusive,
        }),
        valid_time: interval("2026-01-01T00:00:00Z", "2026-03-31T23:59:59Z"),
        ..global_claim("ACME 2026", "ent:alpha", "source_a")
    };
    let second = AliasClaim {
        trusted_anchor: Some(TrustedAnchor {
            namespace: "loan_id".to_string(),
            value: "L-100".to_string(),
            exclusivity: AnchorExclusivity::Exclusive,
        }),
        valid_time: interval("2026-06-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time("2026-06-02T00:00:00Z", 5),
        ..global_claim("ACME 2027", "ent:beta", "source_a")
    };

    let gap_snapshot = compile_alias_snapshot(
        &[first.clone(), second.clone()],
        "2026-04-15T00:00:00Z",
        "2026-07-01T00:00:00Z",
    )
    .expect("gap snapshot compiles");
    assert!(gap_snapshot.active_claims.is_empty());

    let late_snapshot = compile_alias_snapshot(
        &[first.clone(), second.clone()],
        "2026-07-01T00:00:00Z",
        "2026-07-01T00:00:00Z",
    )
    .expect("late snapshot compiles");
    assert_eq!(late_snapshot.active_claims.len(), 1);
    assert_eq!(late_snapshot.active_claims[0].entity_id, "ent:beta");

    let artifact = compile_conflict_artifact(
        &[first, second],
        ConflictPolicy {
            version: CANON_TEMPORAL_CONFLICT_POLICY_VERSION.to_string(),
            policy_id: "recycle-ok".to_string(),
            clauses: vec![ConflictPolicyClause {
                clause_id: "allow-recycled-anchor".to_string(),
                conflict_class: ConflictClass::RecycledIdentifier,
                resolution: ConflictResolution::AllowHistoricalReuse,
            }],
        },
        "2026-07-01T00:00:00Z",
        "2026-07-01T00:00:00Z",
    )
    .expect("recycled artifact compiles");
    let recycled =
        find_conflict(&artifact, ConflictClass::RecycledIdentifier).expect("recycled conflict");
    assert_eq!(recycled.disposition, ConflictDisposition::HistoricalOnly);
    assert_eq!(
        recycled.policy_clause_ids_used,
        vec!["allow-recycled-anchor".to_string()]
    );
}

fn find_conflict(
    artifact: &ConflictArtifact,
    class: ConflictClass,
) -> Option<&temporal_impl::conflict::ConflictRecord> {
    artifact
        .conflicts
        .iter()
        .find(|conflict| conflict.class == class)
}

fn empty_policy(policy_id: &str) -> ConflictPolicy {
    ConflictPolicy {
        version: CANON_TEMPORAL_CONFLICT_POLICY_VERSION.to_string(),
        policy_id: policy_id.to_string(),
        clauses: Vec::new(),
    }
}

fn source_precedence_policy(
    policy_id: &str,
    class: ConflictClass,
    source_systems: &[&str],
) -> ConflictPolicy {
    ConflictPolicy {
        version: CANON_TEMPORAL_CONFLICT_POLICY_VERSION.to_string(),
        policy_id: policy_id.to_string(),
        clauses: vec![ConflictPolicyClause {
            clause_id: format!("{policy_id}-{class:?}").to_ascii_lowercase(),
            conflict_class: class,
            resolution: ConflictResolution::PreferSourceOrder {
                source_systems: source_systems
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
            },
        }],
    }
}

fn sample_claim(alias_value: &str, entity_id: &str, source_system: &str) -> AliasClaim {
    AliasClaim {
        version: String::new(),
        claim_id: String::new(),
        claim_key: String::new(),
        conflict_key: String::new(),
        alias_value: alias_value.to_string(),
        alias_kind: AliasValueKind::Name,
        entity_id: entity_id.to_string(),
        lookup_visibility: LookupVisibility::SourceScoped,
        scope: AliasScope {
            source_system: Some(source_system.to_string()),
            scope_type: Some("product_line".to_string()),
            scope_id: Some("book_a".to_string()),
        },
        valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time("2026-01-01T00:00:00Z", 1),
        source_locator: SourceLocator {
            source_system: source_system.to_string(),
            locator: format!("fixtures/{source_system}.jsonl"),
            fragment: Some("row-1".to_string()),
        },
        materialization_digest: sample_hash('a'),
        assertion_status: AssertionStatus::Accepted,
        trust_policy_ref: "trust.default.v1".to_string(),
        promoted_to_global_by: None,
        trusted_anchor: None,
        supersedes: Vec::new(),
        retracts: Vec::new(),
    }
}

fn global_claim(alias_value: &str, entity_id: &str, source_system: &str) -> AliasClaim {
    AliasClaim {
        lookup_visibility: LookupVisibility::Global,
        scope: AliasScope::default(),
        ..sample_claim(alias_value, entity_id, source_system)
    }
}

fn interval(start_at: &str, end_at: &str) -> TimeInterval {
    TimeInterval {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: Some(end_at.to_string()),
        end_bound: IntervalBoundary::Inclusive,
    }
}

fn recorded_time(start_at: &str, transaction_seq: u64) -> RecordedTime {
    RecordedTime {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: None,
        end_bound: IntervalBoundary::Open,
        transaction_seq: Some(transaction_seq),
    }
}

fn sample_hash(byte: char) -> String {
    format!("blake3:{}", byte.to_string().repeat(64))
}
