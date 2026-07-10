#![forbid(unsafe_code)]

#[path = "../src/extensions/evidence_policy.rs"]
mod evidence_policy;

use evidence_policy::{
    AbstentionDisposition, AbstentionRule, AutoDecisionOutcome, AutoDecisionRule, CandidateRule,
    CandidateTargetKind, CompiledEvidencePolicy, DecisionGate, DecisionOutcome,
    EvidenceCapabilityCatalog, EvidenceIrKind, EvidencePolicyCompatibility,
    EvidencePolicyDefinition, EvidencePolicyDocumentationRef, EvidencePolicyErrorCode,
    EvidencePolicyPackage, EvidencePolicyRef, EvidencePrimitive, EvidenceRule, EvidenceRuleLane,
    EvidenceSelectorScope, ReviewDisposition, ReviewRule, canonical_package_bytes, compile_policy,
    evaluate_triggered_rules, evidence_policy_package_digest, evidence_policy_schema_version,
    finalize_package, package_compatibility, resolve_policy_ref, validate_package_for_execution,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.evidence.policy.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/evidence_policy.rs");

#[test]
fn schema_declares_explicit_evidence_policy_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.evidence.policy.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        "canon.evidence.policy.v1"
    );
    assert_eq!(
        schema["$defs"]["policy_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["opaque_ref"]["pattern"],
        "^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
    );
    assert_eq!(
        schema["x-canon-contract"]["candidate_generation_separate_from_evidence_emission"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["typed_evidence_ir_required"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["implicit_weights_forbidden"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["hard_veto_and_auto_decision_authority_explicit"],
        true
    );
    assert!(!SCHEMA_JSON.contains("\"weight\""));
    assert_eq!(evidence_policy_schema_version(), "canon.evidence.policy.v1");
}

#[test]
fn unknown_policy_and_opaque_refs_work_without_dispatch_changes() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = evidence_policy_package_digest(&package).expect("digest computes");
    let reference = EvidencePolicyRef {
        package_digest: digest.clone(),
        policy_id: "pkg.synthetic:default".to_string(),
    };
    let resolved = resolve_policy_ref(&package, &reference).expect("policy resolves");
    assert_eq!(resolved.policy_id, "pkg.synthetic:default");
    assert_eq!(
        validate_package_for_execution(
            &package,
            std::slice::from_ref(&reference),
            &supported_capabilities()
        )
        .expect("package validates"),
        digest
    );
    let compiled =
        compile_policy(&package, &reference, &supported_capabilities()).expect("policy compiles");
    assert_eq!(compiled.policy.policy_id, "pkg.synthetic:default");
}

#[test]
fn evidence_lanes_stay_separate_and_hard_veto_overrides_other_triggers() {
    let compiled = compiled_policy();
    let lanes = compiled
        .policy
        .evidence_rules
        .iter()
        .map(|rule| (rule.rule_id.clone(), rule.lane, rule.emits_kind))
        .collect::<Vec<_>>();
    assert!(lanes.contains(&(
        "pkg.synthetic:positive_exact_name".to_string(),
        EvidenceRuleLane::Positive,
        EvidenceIrKind::PairSupport
    )));
    assert!(lanes.contains(&(
        "pkg.synthetic:negative_anchor_conflict".to_string(),
        EvidenceRuleLane::Negative,
        EvidenceIrKind::ContextualNegative
    )));
    assert!(lanes.contains(&(
        "pkg.synthetic:context_shared_relation".to_string(),
        EvidenceRuleLane::Contextual,
        EvidenceIrKind::ContextOnly
    )));
    assert!(lanes.contains(&(
        "pkg.synthetic:missing_anchor".to_string(),
        EvidenceRuleLane::Missing,
        EvidenceIrKind::Missingness
    )));
    assert!(lanes.contains(&(
        "pkg.synthetic:hard_veto_protected_conflict".to_string(),
        EvidenceRuleLane::HardVeto,
        EvidenceIrKind::AntiMergeVeto
    )));

    let decision = evaluate_triggered_rules(
        &compiled,
        &[
            "pkg.synthetic:positive_exact_name".to_string(),
            "pkg.synthetic:hard_veto_protected_conflict".to_string(),
        ],
    )
    .expect("decision evaluates");
    assert_eq!(decision.outcome, DecisionOutcome::HardVeto);
    assert_eq!(
        decision.matched_rule_ids,
        vec!["pkg.synthetic:hard_veto_protected_conflict".to_string()]
    );
}

#[test]
fn auto_decision_abstention_and_review_are_explicit_clauses_not_weights() {
    let compiled = compiled_policy();

    let auto_merge = evaluate_triggered_rules(
        &compiled,
        &["pkg.synthetic:positive_exact_name".to_string()],
    )
    .expect("auto merge evaluates");
    assert_eq!(auto_merge.outcome, DecisionOutcome::AutoMerge);
    assert_eq!(
        auto_merge.authority_id.as_deref(),
        Some("pkg.synthetic:auto_merge")
    );
    assert_eq!(
        auto_merge.gate_id.as_deref(),
        Some("pkg.synthetic:auto_merge_gate")
    );

    let abstain = evaluate_triggered_rules(
        &compiled,
        &["pkg.synthetic:negative_anchor_conflict".to_string()],
    )
    .expect("abstention evaluates");
    assert_eq!(abstain.outcome, DecisionOutcome::AbstainConflict);
    assert_eq!(
        abstain.authority_id.as_deref(),
        Some("pkg.synthetic:abstain_conflict")
    );

    let review = evaluate_triggered_rules(
        &compiled,
        &[
            "pkg.synthetic:context_shared_relation".to_string(),
            "pkg.synthetic:missing_anchor".to_string(),
        ],
    )
    .expect("review evaluates");
    assert_eq!(review.outcome, DecisionOutcome::ReviewRequired);
    assert_eq!(
        review.review_artifact_family.as_deref(),
        Some("review.synthetic.evidence")
    );
    assert_eq!(
        review.allowed_review_dispositions,
        vec![
            ReviewDisposition::Same,
            ReviewDisposition::Distinct,
            ReviewDisposition::Related,
            ReviewDisposition::Uncertain
        ]
    );
}

#[test]
fn unsupported_capabilities_are_rejected_at_plan_time() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let reference = EvidencePolicyRef {
        package_digest: evidence_policy_package_digest(&package).unwrap(),
        policy_id: "pkg.synthetic:default".to_string(),
    };
    let mut capabilities = supported_capabilities();
    capabilities.view_refs = vec!["view.synthetic:legal_core".to_string()];

    let error =
        compile_policy(&package, &reference, &capabilities).expect_err("missing view must fail");
    assert_eq!(error.code, EvidencePolicyErrorCode::UnsupportedCapability);
}

#[test]
fn same_major_package_updates_are_compatible() {
    let locked =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("locked finalizes");
    let locked_digest = evidence_policy_package_digest(&locked).expect("digest computes");
    let reference = EvidencePolicyRef {
        package_digest: locked_digest,
        policy_id: "pkg.synthetic:default".to_string(),
    };

    let mut candidate = sample_package("pkg.synthetic", "1.4.0");
    candidate.policies[0].evidence_rules[0].reason_code = "exact_surface_match_v2".to_string();
    let candidate = finalize_package(candidate).expect("candidate finalizes");

    assert_eq!(
        package_compatibility(&locked, &candidate, &[reference]).expect("same major compatible"),
        EvidencePolicyCompatibility::CompatibleSameMajor
    );
}

#[test]
fn canonical_package_bytes_are_stable_across_input_order() {
    let left = finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("left finalizes");
    let right = finalize_package(shuffled_sample_package("pkg.synthetic", "1.2.3"))
        .expect("right finalizes");

    let left_bytes = canonical_package_bytes(&left).expect("left serializes");
    let right_bytes = canonical_package_bytes(&right).expect("right serializes");
    assert_eq!(left_bytes, right_bytes);
    assert_eq!(
        evidence_policy_package_digest(&left).unwrap(),
        evidence_policy_package_digest(&right).unwrap()
    );
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_evidence_policy_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "evidence policy module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "evidence policy schema should not embed domain term {banned}"
        );
    }
}

fn compiled_policy() -> CompiledEvidencePolicy {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = evidence_policy_package_digest(&package).expect("digest computes");
    compile_policy(
        &package,
        &EvidencePolicyRef {
            package_digest: digest,
            policy_id: "pkg.synthetic:default".to_string(),
        },
        &supported_capabilities(),
    )
    .expect("policy compiles")
}

fn sample_package(package_id: &str, package_version: &str) -> EvidencePolicyPackage {
    EvidencePolicyPackage {
        version: String::new(),
        package_id: package_id.to_string(),
        package_version: package_version.to_string(),
        policies: vec![EvidencePolicyDefinition {
            policy_id: format!("{package_id}:default"),
            candidate_rules: vec![CandidateRule {
                rule_id: format!("{package_id}:candidate_exact_name"),
                primitive: EvidencePrimitive::Exact,
                target_kind: CandidateTargetKind::Pair,
                selectors: EvidenceSelectorScope {
                    field_refs: vec!["field.synthetic:name_raw".to_string()],
                    view_refs: vec!["view.synthetic:core_name".to_string()],
                    namespace_refs: vec![],
                    relation_refs: vec![],
                },
            }],
            evidence_rules: vec![
                EvidenceRule {
                    rule_id: format!("{package_id}:positive_exact_name"),
                    primitive: EvidencePrimitive::Exact,
                    lane: EvidenceRuleLane::Positive,
                    emits_kind: EvidenceIrKind::PairSupport,
                    selectors: EvidenceSelectorScope {
                        field_refs: vec![],
                        view_refs: vec!["view.synthetic:core_name".to_string()],
                        namespace_refs: vec![],
                        relation_refs: vec![],
                    },
                    reason_code: "exact_surface_match".to_string(),
                },
                EvidenceRule {
                    rule_id: format!("{package_id}:negative_anchor_conflict"),
                    primitive: EvidencePrimitive::Anchor,
                    lane: EvidenceRuleLane::Negative,
                    emits_kind: EvidenceIrKind::ContextualNegative,
                    selectors: EvidenceSelectorScope {
                        field_refs: vec![],
                        view_refs: vec![],
                        namespace_refs: vec!["namespace.synthetic:lei".to_string()],
                        relation_refs: vec![],
                    },
                    reason_code: "conflicting_anchor".to_string(),
                },
                EvidenceRule {
                    rule_id: format!("{package_id}:context_shared_relation"),
                    primitive: EvidencePrimitive::Context,
                    lane: EvidenceRuleLane::Contextual,
                    emits_kind: EvidenceIrKind::ContextOnly,
                    selectors: EvidenceSelectorScope {
                        field_refs: vec![],
                        view_refs: vec![],
                        namespace_refs: vec![],
                        relation_refs: vec!["relation.synthetic:coholding".to_string()],
                    },
                    reason_code: "shared_relation_context".to_string(),
                },
                EvidenceRule {
                    rule_id: format!("{package_id}:missing_anchor"),
                    primitive: EvidencePrimitive::Anchor,
                    lane: EvidenceRuleLane::Missing,
                    emits_kind: EvidenceIrKind::Missingness,
                    selectors: EvidenceSelectorScope {
                        field_refs: vec![],
                        view_refs: vec![],
                        namespace_refs: vec!["namespace.synthetic:lei".to_string()],
                        relation_refs: vec![],
                    },
                    reason_code: "anchor_missing".to_string(),
                },
                EvidenceRule {
                    rule_id: format!("{package_id}:hard_veto_protected_conflict"),
                    primitive: EvidencePrimitive::ProtectedConflict,
                    lane: EvidenceRuleLane::HardVeto,
                    emits_kind: EvidenceIrKind::AntiMergeVeto,
                    selectors: EvidenceSelectorScope {
                        field_refs: vec![],
                        view_refs: vec!["view.synthetic:legal_core".to_string()],
                        namespace_refs: vec![],
                        relation_refs: vec![],
                    },
                    reason_code: "protected_conflict".to_string(),
                },
            ],
            decision_gates: vec![
                DecisionGate {
                    gate_id: format!("{package_id}:auto_merge_gate"),
                    require_rule_ids: vec![format!("{package_id}:positive_exact_name")],
                    forbid_rule_ids: vec![],
                    minimum_trigger_count: Some(1),
                },
                DecisionGate {
                    gate_id: format!("{package_id}:abstain_conflict_gate"),
                    require_rule_ids: vec![format!("{package_id}:negative_anchor_conflict")],
                    forbid_rule_ids: vec![],
                    minimum_trigger_count: Some(1),
                },
                DecisionGate {
                    gate_id: format!("{package_id}:review_gate"),
                    require_rule_ids: vec![
                        format!("{package_id}:context_shared_relation"),
                        format!("{package_id}:missing_anchor"),
                    ],
                    forbid_rule_ids: vec![],
                    minimum_trigger_count: Some(2),
                },
            ],
            auto_decision_rules: vec![AutoDecisionRule {
                decision_id: format!("{package_id}:auto_merge"),
                gate_id: format!("{package_id}:auto_merge_gate"),
                outcome: AutoDecisionOutcome::Merge,
                authority_label: "explicit_auto_merge".to_string(),
            }],
            abstention_rules: vec![AbstentionRule {
                abstention_id: format!("{package_id}:abstain_conflict"),
                gate_id: format!("{package_id}:abstain_conflict_gate"),
                disposition: AbstentionDisposition::Conflict,
                reason_code: "negative_evidence_conflict".to_string(),
            }],
            review_rules: vec![ReviewRule {
                review_id: format!("{package_id}:review_contextual_gap"),
                gate_id: format!("{package_id}:review_gate"),
                artifact_family: "review.synthetic.evidence".to_string(),
                allowed_dispositions: vec![
                    ReviewDisposition::Same,
                    ReviewDisposition::Distinct,
                    ReviewDisposition::Related,
                    ReviewDisposition::Uncertain,
                ],
            }],
            documentation_refs: vec!["docs/evidence-policy.md".to_string()],
        }],
        documentation: vec![EvidencePolicyDocumentationRef {
            label: "Evidence Policy Guide".to_string(),
            uri: "docs/evidence-policy.md".to_string(),
        }],
    }
}

fn shuffled_sample_package(package_id: &str, package_version: &str) -> EvidencePolicyPackage {
    let mut package = sample_package(package_id, package_version);
    package.policies[0].candidate_rules.reverse();
    package.policies[0].evidence_rules.reverse();
    package.policies[0].decision_gates.reverse();
    package.policies[0].review_rules.reverse();
    package
}

fn supported_capabilities() -> EvidenceCapabilityCatalog {
    EvidenceCapabilityCatalog {
        field_refs: vec!["field.synthetic:name_raw".to_string()],
        view_refs: vec![
            "view.synthetic:core_name".to_string(),
            "view.synthetic:legal_core".to_string(),
        ],
        namespace_refs: vec!["namespace.synthetic:lei".to_string()],
        relation_refs: vec!["relation.synthetic:coholding".to_string()],
        supported_primitives: vec![
            EvidencePrimitive::Exact,
            EvidencePrimitive::Anchor,
            EvidencePrimitive::Context,
            EvidencePrimitive::ProtectedConflict,
        ],
        supported_candidate_targets: vec![CandidateTargetKind::Pair],
        supported_evidence_kinds: vec![
            EvidenceIrKind::PairSupport,
            EvidenceIrKind::ContextualNegative,
            EvidenceIrKind::ContextOnly,
            EvidenceIrKind::Missingness,
            EvidenceIrKind::AntiMergeVeto,
        ],
    }
}
