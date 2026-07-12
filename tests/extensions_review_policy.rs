#![forbid(unsafe_code)]

pub use canon::{Refusal, entity};

#[path = "../src/entity/review_export.rs"]
#[allow(dead_code)]
mod native_review_export;
#[path = "../src/extensions/review_policy.rs"]
mod review_policy;

use canon::entity::{
    EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
    EntityRegistrySnapshot, EntityStrategyReference,
    review::{ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem, ReviewRelationHint},
    review_import::{
        NativeReviewDecision, NativeReviewDecisionAction, NativeReviewDecisionContext,
        NativeReviewDecisionMode, import_native_review_decisions,
        native_review_import_context_from_artifact,
    },
    score::ScoreUnits,
    solve::{SolveEvidenceCut, SolveReconciliationState},
};
use native_review_export::{
    NativeReviewExportRequest, build_native_review_artifact, render_native_review_html,
};
use review_policy::{
    ReviewActionRule, ReviewApproval, ReviewEvidenceGroup, ReviewEvidenceKind, ReviewEvidenceRef,
    ReviewLabelMapping, ReviewPolicyDecisionInput, ReviewPolicyDefinition,
    ReviewPolicyDocumentationRef, ReviewPolicyErrorCode, ReviewPolicyPackage,
    ReviewPolicyPatchKind, ReviewPolicyRef, ReviewRefKind, ReviewRiskTier, ReviewSafeAction,
    ReviewTwoPersonRule, canonical_package_bytes, compile_policy, compile_review_decision,
    finalize_package, package_compatibility, present_evidence_refs, render_opaque_ref,
    review_policy_package_digest, review_policy_schema_version, validate_package_for_execution,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.review.policy.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/review_policy.rs");

#[test]
fn schema_declares_review_policy_boundary_and_safe_actions() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.review.policy.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        "canon.review.policy.v1"
    );
    assert_eq!(
        schema["$defs"]["policy_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["x-canon-contract"]["safe_actions"],
        serde_json::json!([
            "same",
            "distinct",
            "related",
            "successor",
            "alias_scope",
            "assignment",
            "new_entity",
            "defer",
            "reject"
        ])
    );
    assert_eq!(
        schema["x-canon-contract"]["presentation_only"],
        serde_json::json!(true)
    );
    assert_eq!(
        schema["x-canon-contract"]["relation_only_evidence_never_implies_identity"],
        serde_json::json!(true)
    );
    assert_eq!(
        schema["x-canon-contract"]["high_risk_overrides_require_hash_bound_approval"],
        serde_json::json!(true)
    );
    assert_eq!(review_policy_schema_version(), "canon.review.policy.v1");
}

#[test]
fn labels_and_unknown_refs_render_without_dropping_opaque_values() {
    let compiled = compiled_policy();
    let known = render_opaque_ref(
        &compiled,
        ReviewRefKind::Namespace,
        "namespace.synthetic:tax",
    );
    assert!(known.known);
    assert_eq!(known.label, "Tax identifier");

    let unknown = render_opaque_ref(
        &compiled,
        ReviewRefKind::Relation,
        "relation.synthetic:future",
    );
    assert!(!unknown.known);
    assert_eq!(unknown.label, "relation.synthetic:future");

    let presented = present_evidence_refs(
        &compiled,
        &[
            evidence(
                "evidence:known",
                ReviewEvidenceKind::Identity,
                Some("namespace.synthetic:tax"),
                None,
                None,
            ),
            evidence(
                "evidence:unknown",
                ReviewEvidenceKind::Context,
                Some("namespace.synthetic:unmapped"),
                Some("relation.synthetic:future"),
                Some("ontology.synthetic:unmapped"),
            ),
        ],
    )
    .expect("presentation renders");
    assert_eq!(presented.len(), 2);
    assert_eq!(presented[0].group_label, "Identity evidence");
    assert_eq!(
        presented[1].namespace.as_ref().expect("namespace").label,
        "namespace.synthetic:unmapped"
    );
    assert_eq!(
        presented[1].relation.as_ref().expect("relation").label,
        "relation.synthetic:future"
    );
    assert_eq!(
        presented[1].ontology.as_ref().expect("ontology").label,
        "ontology.synthetic:unmapped"
    );
}

#[test]
fn relation_only_evidence_cannot_compile_identity_actions() {
    let package = sample_package("pkg.synthetic", "1.2.3");
    let digest = review_policy_package_digest(&package).expect("digest");
    let reference = policy_ref(&digest);

    let error = compile_review_decision(
        &package,
        &reference,
        &decision(
            &digest,
            "review:relation_only_identity",
            ReviewSafeAction::Same,
            vec![evidence(
                "evidence:relation_only",
                ReviewEvidenceKind::RelationOnly,
                None,
                Some("relation.synthetic:shared_context"),
                None,
            )],
        ),
    )
    .expect_err("relation-only identity refuses");
    assert_eq!(error.code, ReviewPolicyErrorCode::RelationIdentityBoundary);
}

#[test]
fn high_risk_actions_require_hash_bound_approvals() {
    let package = sample_package("pkg.synthetic", "1.2.3");
    let digest = review_policy_package_digest(&package).expect("digest");
    let reference = policy_ref(&digest);
    let mut input = decision(
        &digest,
        "review:distinct",
        ReviewSafeAction::Distinct,
        vec![evidence(
            "evidence:distinct",
            ReviewEvidenceKind::Distinct,
            Some("namespace.synthetic:tax"),
            None,
            None,
        )],
    );
    input.rationale = Some("conflicting protected identifier".to_string());

    let error = compile_review_decision(&package, &reference, &input)
        .expect_err("approval missing refuses");
    assert_eq!(error.code, ReviewPolicyErrorCode::ApprovalRequired);

    input.approvals = vec![ReviewApproval {
        approval_id: "approval:bad".to_string(),
        operator_id: "operator:principal".to_string(),
        role_ref: "role.synthetic:principal".to_string(),
        approved_action: ReviewSafeAction::Distinct,
        policy_digest: digest.clone(),
        decision_binding_hash: sample_hash('b'),
    }];
    let error = compile_review_decision(&package, &reference, &input)
        .expect_err("wrong decision hash refuses");
    assert_eq!(error.code, ReviewPolicyErrorCode::ApprovalRequired);

    input.approvals[0].decision_binding_hash = sample_hash('d');
    let patch =
        compile_review_decision(&package, &reference, &input).expect("hash-bound approval accepts");
    assert_eq!(patch.patch_kind, ReviewPolicyPatchKind::CannotLink);
    assert!(patch.approvals_hash.starts_with("blake3:"));
}

#[test]
fn safe_actions_compile_to_typed_patches_without_changing_review_format() {
    let package = sample_package("pkg.synthetic", "1.2.3");
    let digest = review_policy_package_digest(&package).expect("digest");
    let reference = policy_ref(&digest);
    let cases = [
        (ReviewSafeAction::Same, ReviewPolicyPatchKind::Alias),
        (
            ReviewSafeAction::Distinct,
            ReviewPolicyPatchKind::CannotLink,
        ),
        (ReviewSafeAction::Related, ReviewPolicyPatchKind::Relation),
        (
            ReviewSafeAction::Successor,
            ReviewPolicyPatchKind::SuccessorRelation,
        ),
        (
            ReviewSafeAction::AliasScope,
            ReviewPolicyPatchKind::AliasScope,
        ),
        (
            ReviewSafeAction::Assignment,
            ReviewPolicyPatchKind::Assignment,
        ),
        (
            ReviewSafeAction::NewEntity,
            ReviewPolicyPatchKind::NewEntity,
        ),
        (ReviewSafeAction::Defer, ReviewPolicyPatchKind::Defer),
        (ReviewSafeAction::Reject, ReviewPolicyPatchKind::Reject),
    ];

    for (action, expected_patch) in cases {
        let mut input = decision(
            &digest,
            &format!("review:{}", action.as_str()),
            action,
            evidence_for_action(action),
        );
        input.rationale = Some(format!("rationale for {}", action.as_str()));
        if action == ReviewSafeAction::Assignment {
            input.target_canonical_id = Some("ENTITY-001".to_string());
        }
        if matches!(
            action,
            ReviewSafeAction::Distinct | ReviewSafeAction::Reject
        ) {
            input.approvals = vec![approval(&digest, action)];
        }
        let patch = compile_review_decision(&package, &reference, &input)
            .unwrap_or_else(|error| panic!("action {action:?} compiles: {error}"));
        assert_eq!(patch.patch_kind, expected_patch);
        assert_eq!(patch.action, action);
        assert_eq!(patch.policy_digest, digest);
    }

    let review = build_native_review_artifact(NativeReviewExportRequest {
        review_queue: review_queue(),
        run_content_hash: sample_hash('r'),
        policy_content_hash: digest.clone(),
    })
    .expect("shared native review builds");
    let html = render_native_review_html(&review).expect("shared html renders");
    assert!(html.contains("Canon Entity Review"));

    let native_value = serde_json::to_value(&review).expect("native value");
    let native_context =
        native_review_import_context_from_artifact(&native_value).expect("native context");
    let native_item = &native_value["review_items"][0];
    let native_decision = NativeReviewDecision {
        review_id: native_item["review_id"].as_str().unwrap().to_string(),
        mode: NativeReviewDecisionMode::Link,
        action: NativeReviewDecisionAction::Relation,
        operator_id: "operator:reviewer".to_string(),
        reason_code: "related".to_string(),
        note: "shared renderer/importer remains canonical".to_string(),
        source_review_artifact_hash: review.artifact_content_hash.clone(),
        decision_binding_hash: native_item["decision_binding_hash"]
            .as_str()
            .unwrap()
            .to_string(),
        run_content_hash: sample_hash('r'),
        policy_content_hash: digest,
        registry_snapshot_hash: sample_hash('g'),
        mode_context: serde_json::from_value::<NativeReviewDecisionContext>(
            native_item["mode_context"].clone(),
        )
        .expect("mode context"),
        surface_ids: vec![],
        target_canonical_id: None,
        relation: Some("related".to_string()),
    };
    let receipt = import_native_review_decisions(native_context, vec![native_decision])
        .expect("shared native import accepts relation action");
    assert_eq!(receipt.patches.relation_patches.len(), 1);
}

#[test]
fn package_digest_is_stable_and_same_major_updates_are_compatible() {
    let locked = sample_package("pkg.synthetic", "1.2.3");
    let locked_digest = review_policy_package_digest(&locked).expect("locked digest");
    let reference = policy_ref(&locked_digest);
    let mut candidate = shuffled_sample_package("pkg.synthetic", "1.4.0");
    candidate.labels[0].label = "Updated tax identifier".to_string();
    let candidate = finalize_package(candidate).expect("candidate finalizes");

    assert_eq!(
        package_compatibility(&locked, &candidate, &[reference]).expect("same major compatible"),
        review_policy::ReviewPolicyCompatibility::CompatibleSameMajor
    );

    let left = canonical_package_bytes(&locked).expect("left bytes");
    let right =
        canonical_package_bytes(&sample_package("pkg.synthetic", "1.2.3")).expect("right bytes");
    assert_eq!(left, right);
    assert_eq!(
        validate_package_for_execution(&locked, &[policy_ref(&locked_digest)])
            .expect("execution validates"),
        locked_digest
    );
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_review_policy_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "review policy module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "review policy schema should not embed domain term {banned}"
        );
    }
}

fn compiled_policy() -> review_policy::CompiledReviewPolicy {
    let package = sample_package("pkg.synthetic", "1.2.3");
    let digest = review_policy_package_digest(&package).expect("digest");
    compile_policy(&package, &policy_ref(&digest)).expect("policy compiles")
}

fn sample_package(package_id: &str, package_version: &str) -> ReviewPolicyPackage {
    finalize_package(ReviewPolicyPackage {
        version: String::new(),
        package_id: package_id.to_string(),
        package_version: package_version.to_string(),
        labels: vec![
            ReviewLabelMapping {
                ref_id: "namespace.synthetic:tax".to_string(),
                ref_kind: ReviewRefKind::Namespace,
                label: "Tax identifier".to_string(),
                help_text: Some("Protected identifier namespace".to_string()),
            },
            ReviewLabelMapping {
                ref_id: "relation.synthetic:shared_context".to_string(),
                ref_kind: ReviewRefKind::Relation,
                label: "Shared context".to_string(),
                help_text: None,
            },
            ReviewLabelMapping {
                ref_id: "relation.synthetic:successor".to_string(),
                ref_kind: ReviewRefKind::Relation,
                label: "Successor relation".to_string(),
                help_text: None,
            },
        ],
        documentation: vec![ReviewPolicyDocumentationRef {
            label: "review package".to_string(),
            uri: "docs/review-policy.md".to_string(),
        }],
        policies: vec![ReviewPolicyDefinition {
            policy_id: format!("{package_id}:default"),
            evidence_groups: vec![
                ReviewEvidenceGroup {
                    group_id: "group.synthetic:identity".to_string(),
                    label: "Identity evidence".to_string(),
                    namespace_refs: vec!["namespace.synthetic:tax".to_string()],
                    relation_refs: vec![],
                    ontology_refs: vec![],
                    evidence_kinds: vec![ReviewEvidenceKind::Identity],
                    risk_tier: ReviewRiskTier::Low,
                    required_rationale: false,
                },
                ReviewEvidenceGroup {
                    group_id: "group.synthetic:relationship".to_string(),
                    label: "Relationship evidence".to_string(),
                    namespace_refs: vec![],
                    relation_refs: vec![
                        "relation.synthetic:shared_context".to_string(),
                        "relation.synthetic:successor".to_string(),
                    ],
                    ontology_refs: vec![],
                    evidence_kinds: vec![
                        ReviewEvidenceKind::RelationOnly,
                        ReviewEvidenceKind::Succession,
                    ],
                    risk_tier: ReviewRiskTier::Medium,
                    required_rationale: true,
                },
                ReviewEvidenceGroup {
                    group_id: "group.synthetic:conflict".to_string(),
                    label: "Conflict evidence".to_string(),
                    namespace_refs: vec!["namespace.synthetic:tax".to_string()],
                    relation_refs: vec![],
                    ontology_refs: vec![],
                    evidence_kinds: vec![
                        ReviewEvidenceKind::Distinct,
                        ReviewEvidenceKind::Override,
                    ],
                    risk_tier: ReviewRiskTier::High,
                    required_rationale: true,
                },
            ],
            action_rules: action_rules(),
            documentation_refs: vec!["docs/review-policy.md".to_string()],
        }],
    })
    .expect("package finalizes")
}

fn shuffled_sample_package(package_id: &str, package_version: &str) -> ReviewPolicyPackage {
    let mut package = sample_package(package_id, package_version);
    package.labels.reverse();
    package.policies[0].evidence_groups.reverse();
    package.policies[0].action_rules.reverse();
    package
}

fn action_rules() -> Vec<ReviewActionRule> {
    vec![
        action_rule(
            ReviewSafeAction::Same,
            ReviewPolicyPatchKind::Alias,
            ReviewRiskTier::Low,
            vec![ReviewEvidenceKind::Identity],
        ),
        action_rule(
            ReviewSafeAction::Distinct,
            ReviewPolicyPatchKind::CannotLink,
            ReviewRiskTier::High,
            vec![ReviewEvidenceKind::Distinct, ReviewEvidenceKind::Override],
        ),
        action_rule(
            ReviewSafeAction::Related,
            ReviewPolicyPatchKind::Relation,
            ReviewRiskTier::Medium,
            vec![ReviewEvidenceKind::RelationOnly],
        ),
        action_rule(
            ReviewSafeAction::Successor,
            ReviewPolicyPatchKind::SuccessorRelation,
            ReviewRiskTier::Medium,
            vec![ReviewEvidenceKind::Succession],
        ),
        action_rule(
            ReviewSafeAction::AliasScope,
            ReviewPolicyPatchKind::AliasScope,
            ReviewRiskTier::Low,
            vec![ReviewEvidenceKind::Identity],
        ),
        action_rule(
            ReviewSafeAction::Assignment,
            ReviewPolicyPatchKind::Assignment,
            ReviewRiskTier::Medium,
            vec![ReviewEvidenceKind::Identity],
        ),
        action_rule(
            ReviewSafeAction::NewEntity,
            ReviewPolicyPatchKind::NewEntity,
            ReviewRiskTier::Low,
            vec![ReviewEvidenceKind::Missing],
        ),
        action_rule(
            ReviewSafeAction::Defer,
            ReviewPolicyPatchKind::Defer,
            ReviewRiskTier::Low,
            vec![ReviewEvidenceKind::Context],
        ),
        action_rule(
            ReviewSafeAction::Reject,
            ReviewPolicyPatchKind::Reject,
            ReviewRiskTier::Critical,
            vec![ReviewEvidenceKind::Override],
        ),
    ]
}

fn action_rule(
    action: ReviewSafeAction,
    patch_kind: ReviewPolicyPatchKind,
    risk_tier: ReviewRiskTier,
    allowed_evidence_kinds: Vec<ReviewEvidenceKind>,
) -> ReviewActionRule {
    ReviewActionRule {
        action,
        label: action.as_str().replace('_', " "),
        patch_kind,
        risk_tier,
        required_rationale: matches!(risk_tier, ReviewRiskTier::High | ReviewRiskTier::Critical)
            || matches!(
                action,
                ReviewSafeAction::Related | ReviewSafeAction::Successor
            ),
        allowed_evidence_kinds,
        two_person_rule: risk_tier
            .requires_approval_for_test()
            .then(|| ReviewTwoPersonRule {
                min_approvals: 1,
                approval_role_refs: vec!["role.synthetic:principal".to_string()],
            }),
    }
}

trait TestRiskTier {
    fn requires_approval_for_test(self) -> bool;
}

impl TestRiskTier for ReviewRiskTier {
    fn requires_approval_for_test(self) -> bool {
        matches!(self, ReviewRiskTier::High | ReviewRiskTier::Critical)
    }
}

fn policy_ref(digest: &str) -> ReviewPolicyRef {
    ReviewPolicyRef {
        package_digest: digest.to_string(),
        policy_id: "pkg.synthetic:default".to_string(),
    }
}

fn decision(
    digest: &str,
    review_id: &str,
    action: ReviewSafeAction,
    evidence_refs: Vec<ReviewEvidenceRef>,
) -> ReviewPolicyDecisionInput {
    ReviewPolicyDecisionInput {
        review_id: review_id.to_string(),
        action,
        operator_id: "operator:reviewer".to_string(),
        policy_digest: digest.to_string(),
        source_review_artifact_hash: sample_hash('a'),
        decision_binding_hash: sample_hash('d'),
        surface_ids: vec!["surf:alpha".to_string(), "surf:beta".to_string()],
        relation_ref: None,
        target_canonical_id: None,
        rationale: None,
        evidence_refs,
        approvals: vec![],
    }
}

fn evidence_for_action(action: ReviewSafeAction) -> Vec<ReviewEvidenceRef> {
    match action {
        ReviewSafeAction::Same | ReviewSafeAction::AliasScope | ReviewSafeAction::Assignment => {
            vec![evidence(
                "evidence:identity",
                ReviewEvidenceKind::Identity,
                Some("namespace.synthetic:tax"),
                None,
                None,
            )]
        }
        ReviewSafeAction::Distinct => vec![evidence(
            "evidence:distinct",
            ReviewEvidenceKind::Distinct,
            Some("namespace.synthetic:tax"),
            None,
            None,
        )],
        ReviewSafeAction::Related => vec![evidence(
            "evidence:related",
            ReviewEvidenceKind::RelationOnly,
            None,
            Some("relation.synthetic:shared_context"),
            None,
        )],
        ReviewSafeAction::Successor => vec![evidence(
            "evidence:successor",
            ReviewEvidenceKind::Succession,
            None,
            Some("relation.synthetic:successor"),
            None,
        )],
        ReviewSafeAction::NewEntity => vec![evidence(
            "evidence:missing",
            ReviewEvidenceKind::Missing,
            None,
            None,
            None,
        )],
        ReviewSafeAction::Defer => vec![evidence(
            "evidence:context",
            ReviewEvidenceKind::Context,
            None,
            None,
            None,
        )],
        ReviewSafeAction::Reject => vec![evidence(
            "evidence:override",
            ReviewEvidenceKind::Override,
            None,
            None,
            None,
        )],
    }
}

fn evidence(
    evidence_id: &str,
    evidence_kind: ReviewEvidenceKind,
    namespace_ref: Option<&str>,
    relation_ref: Option<&str>,
    ontology_ref: Option<&str>,
) -> ReviewEvidenceRef {
    ReviewEvidenceRef {
        evidence_id: evidence_id.to_string(),
        evidence_kind,
        namespace_ref: namespace_ref.map(str::to_string),
        relation_ref: relation_ref.map(str::to_string),
        ontology_ref: ontology_ref.map(str::to_string),
        reason_code: Some("synthetic_reason".to_string()),
    }
}

fn approval(digest: &str, action: ReviewSafeAction) -> ReviewApproval {
    ReviewApproval {
        approval_id: format!("approval:{}", action.as_str()),
        operator_id: "operator:principal".to_string(),
        role_ref: "role.synthetic:principal".to_string(),
        approved_action: action,
        policy_digest: digest.to_string(),
        decision_binding_hash: sample_hash('d'),
    }
}

fn review_queue() -> ReviewQueueArtifact {
    ReviewQueueArtifact {
        version: "canon_entity_review_queue.v0".to_string(),
        artifact_content_hash: sample_hash('q'),
        metadata: metadata(),
        summary: canon::entity::EntityDeterministicSummary::default(),
        source_solve_hash: sample_hash('s'),
        source_link_hash: None,
        review_items: vec![ReviewQueueItem {
            review_id: "review:shared_native".to_string(),
            ambiguity_key: "shared_native".to_string(),
            component_id: "component:shared_native".to_string(),
            state: SolveReconciliationState::Escrow,
            proposed_action: "confirm_merge_distinct_or_relation".to_string(),
            review_priority_units: 5000,
            priority_reasons: vec!["related_distinct".to_string()],
            affected_rows: 2,
            affected_deals: 1,
            surface_ids: vec!["surf:alpha".to_string(), "surf:beta".to_string()],
            strongest_positive_cut: Some(SolveEvidenceCut {
                left_surface_id: "surf:alpha".to_string(),
                right_surface_id: "surf:beta".to_string(),
                score_units: ScoreUnits::saturating_from_units(6000),
                evidence_count: 1,
                evidence_reason_codes: vec!["relation_only".to_string()],
            }),
            strongest_negative_cut: None,
            relation_hints: vec![ReviewRelationHint {
                left_surface_id: "surf:alpha".to_string(),
                right_surface_id: "surf:beta".to_string(),
                relation: "related".to_string(),
                reason_code: "related_distinct".to_string(),
            }],
            provenance_samples: vec![ReviewProvenanceSample {
                surface_id: "surf:alpha".to_string(),
                row_id: "row:1".to_string(),
                source: "fixture.csv".to_string(),
                raw_value: "Alpha".to_string(),
            }],
        }],
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "synthetic_entity".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "synthetic".to_string(),
            identity_semantics: "canonical_synthetic".to_string(),
            canonical_type: "synthetic".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "synthetic.aliases".to_string(),
                distinct: "synthetic.distinct".to_string(),
                relations: "synthetic.relations".to_string(),
            },
            content_hash: Some(sample_hash('p')),
        },
        strategy: EntityStrategyReference {
            id: "synthetic_strategy".to_string(),
            version: "0.1.0".to_string(),
            content_hash: sample_hash('t'),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "synthetic-registry".to_string(),
            version: "2026.07.11".to_string(),
            source: "registries/synthetic".to_string(),
            lookup_snapshot_hash: sample_hash('g'),
            sidecar_snapshot_hash: Some(sample_hash('h')),
        },
        patch_namespace: "synthetic.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 2,
            content_hash: sample_hash('i'),
        }),
        upstream_artifacts: Vec::new(),
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn sample_hash(ch: char) -> String {
    format!("blake3:{}", ch.to_string().repeat(64))
}
