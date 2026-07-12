#![forbid(unsafe_code)]

use canon::entity::explain::{
    CANON_ENTITY_EXPLAIN_WATERFALL_VERSION, EvidenceExplainInputRecord, EvidenceExplainPageRequest,
    EvidenceExplainRedactionPolicy, EvidenceExplainSourceArtifact, EvidenceExplanationOutcome,
    EvidenceWaterfallEntryKind, EvidenceWaterfallRequest, render_evidence_waterfall,
    render_evidence_waterfall_summary,
};
use serde_json::Value;

const EXPLAIN_SCHEMA_JSON: &str = include_str!("../schemas/canon.entity.explain.v1.schema.json");

#[test]
fn evidence_waterfall_distinguishes_lanes_redacts_and_explains_veto() {
    let artifact = render_evidence_waterfall(EvidenceWaterfallRequest {
        decision_id: "decision:block-veto".to_string(),
        outcome: EvidenceExplanationOutcome::BlockedByVeto,
        subject_ids: vec!["subject:b".to_string(), "subject:a".to_string()],
        observed_score_units: 7200,
        threshold_score_units: 8000,
        source_artifacts: source_artifacts(),
        evidence_records: vec![
            EvidenceExplainInputRecord::new(
                "evidence",
                "support:legal-name",
                EvidenceWaterfallEntryKind::PositiveScoreContribution,
                5200,
                "legal-name exact view agreed",
            )
            .with_targets(["subject:a", "subject:b"])
            .with_operator("exact_view:legal_name", "exact_name_match"),
            EvidenceExplainInputRecord::new(
                "evidence",
                "veto:reviewed-distinct",
                EvidenceWaterfallEntryKind::Veto,
                -10000,
                "Private Tenant Name cannot link to sealed tenant",
            )
            .with_targets(["subject:a", "subject:b"])
            .with_operator("cannot_link:reviewed_distinctness", "reviewed_distinctness")
            .with_hard_veto()
            .with_sensitive_payload(),
            EvidenceExplainInputRecord::new(
                "evidence",
                "context:weak-geography",
                EvidenceWaterfallEntryKind::Context,
                -500,
                "geography context weakens attachment",
            ),
        ],
        candidate_context: vec![EvidenceExplainInputRecord::new(
            "candidate",
            "candidate:scope-1",
            EvidenceWaterfallEntryKind::CandidateContext,
            0,
            "one competing incumbent remained in the candidate set",
        )],
        solver_decisions: vec![EvidenceExplainInputRecord::new(
            "solver",
            "solve:component-1",
            EvidenceWaterfallEntryKind::SolverDecision,
            0,
            "solver abstained because a hard veto was active",
        )],
        policy_clauses: vec![EvidenceExplainInputRecord::new(
            "policy",
            "policy:cannot-link",
            EvidenceWaterfallEntryKind::PolicyClause,
            0,
            "reviewed cannot-link is a hard veto",
        )],
        registry_facts: vec![EvidenceExplainInputRecord::new(
            "registry",
            "registry:known-id",
            EvidenceWaterfallEntryKind::RegistryFact,
            0,
            "existing canonical id was present in registry snapshot",
        )],
        review_overrides: vec![EvidenceExplainInputRecord::new(
            "review",
            "review:override-1",
            EvidenceWaterfallEntryKind::ReviewOverride,
            0,
            "review override preserved abstention",
        )],
        missing_evidence: vec![EvidenceExplainInputRecord::new(
            "evidence",
            "missing:dba",
            EvidenceWaterfallEntryKind::MissingEvidence,
            0,
            "dba alias view was absent from frozen evidence",
        )],
        redaction: EvidenceExplainRedactionPolicy::default(),
        page: EvidenceExplainPageRequest::default(),
    })
    .expect("waterfall renders");

    assert_eq!(artifact.version, CANON_ENTITY_EXPLAIN_WATERFALL_VERSION);
    assert_eq!(artifact.subject_ids, vec!["subject:a", "subject:b"]);
    assert_eq!(
        artifact.summary.counts_by_lane["positive_score_contributions"],
        1
    );
    assert_eq!(artifact.summary.counts_by_lane["vetoes"], 1);
    assert_eq!(artifact.summary.counts_by_lane["context"], 1);
    assert_eq!(artifact.summary.counts_by_lane["missing_evidence"], 1);
    assert_eq!(artifact.summary.counts_by_lane["review_overrides"], 1);
    assert_eq!(artifact.summary.positive_score_units, 5200);
    assert_eq!(artifact.summary.negative_score_units, -10500);
    assert_eq!(
        artifact.summary.counterfactual.policy_decision,
        "blocked_by_veto"
    );
    assert_eq!(
        artifact
            .summary
            .counterfactual
            .additional_score_units_to_threshold,
        Some(800)
    );
    assert_eq!(artifact.summary.counterfactual.hard_veto_count, 1);
    assert_eq!(artifact.summary.counterfactual.competing_context_count, 1);

    let veto_entry = &artifact.lanes["vetoes"].entries[0];
    assert!(veto_entry.redacted);
    assert!(veto_entry.summary.starts_with("redacted:blake3:"));
    assert!(
        !serde_json::to_string(&artifact)
            .unwrap()
            .contains("Private Tenant Name")
    );
    assert!(artifact.human_summary.contains("outcome=blocked_by_veto"));
    assert_eq!(
        render_evidence_waterfall_summary(&artifact),
        artifact.human_summary
    );
}

#[test]
fn evidence_waterfall_covers_declared_outcome_classes() {
    let cases = [
        (
            EvidenceExplanationOutcome::ExactExisting,
            "threshold_satisfied",
        ),
        (
            EvidenceExplanationOutcome::NewCluster,
            "threshold_satisfied",
        ),
        (EvidenceExplanationOutcome::Linked, "threshold_satisfied"),
        (EvidenceExplanationOutcome::Ambiguous, "ambiguous"),
        (EvidenceExplanationOutcome::Contradictory, "contradictory"),
        (
            EvidenceExplanationOutcome::BlockedByVeto,
            "threshold_satisfied",
        ),
        (
            EvidenceExplanationOutcome::BelowThreshold,
            "below_threshold",
        ),
        (
            EvidenceExplanationOutcome::ReviewOverridden,
            "review_overridden",
        ),
    ];

    for (outcome, expected_policy_decision) in cases {
        let artifact = render_evidence_waterfall(EvidenceWaterfallRequest {
            decision_id: format!("decision:{outcome:?}"),
            outcome,
            subject_ids: vec!["subject:x".to_string()],
            observed_score_units: 7000,
            threshold_score_units: 8000,
            source_artifacts: source_artifacts(),
            evidence_records: vec![EvidenceExplainInputRecord::new(
                "evidence",
                "support:one",
                EvidenceWaterfallEntryKind::PositiveScoreContribution,
                7000,
                "supporting evidence contributed score",
            )],
            candidate_context: Vec::new(),
            solver_decisions: vec![EvidenceExplainInputRecord::new(
                "solver",
                "solve:decision",
                EvidenceWaterfallEntryKind::SolverDecision,
                0,
                "solver emitted the final decision class",
            )],
            policy_clauses: Vec::new(),
            registry_facts: Vec::new(),
            review_overrides: Vec::new(),
            missing_evidence: Vec::new(),
            redaction: EvidenceExplainRedactionPolicy::default(),
            page: EvidenceExplainPageRequest::default(),
        })
        .expect("outcome waterfall renders");

        assert_eq!(artifact.outcome, outcome);
        assert_eq!(
            artifact.summary.counterfactual.policy_decision,
            expected_policy_decision
        );
    }
}

#[test]
fn evidence_waterfall_paginates_deterministically_and_is_byte_stable() {
    let descending = (0..8)
        .rev()
        .map(|index| {
            EvidenceExplainInputRecord::new(
                "evidence",
                format!("support:{index:02}"),
                EvidenceWaterfallEntryKind::PositiveScoreContribution,
                100 + index,
                format!("support contribution {index}"),
            )
        })
        .collect::<Vec<_>>();
    let mut ascending = descending.clone();
    ascending.reverse();

    let first = render_evidence_waterfall(page_request(descending)).expect("first page renders");
    let second = render_evidence_waterfall(page_request(ascending)).expect("second page renders");

    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(first.pagination.page, 0);
    assert_eq!(first.pagination.per_page, 3);
    assert_eq!(first.pagination.total_entries, 8);
    assert_eq!(first.pagination.page_entries, 3);
    assert_eq!(first.pagination.next_page, Some(1));
    assert_eq!(
        first.lanes["positive_score_contributions"]
            .entries
            .iter()
            .map(|entry| entry.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn evidence_waterfall_rejects_unfrozen_or_unpageable_requests() {
    let mut request = page_request(Vec::new());
    request.source_artifacts.clear();
    let error = render_evidence_waterfall(request).expect_err("source artifact required");
    assert_eq!(error.field, "source_artifacts");

    let mut request = page_request(Vec::new());
    request.page.per_page = 0;
    let error = render_evidence_waterfall(request).expect_err("page size required");
    assert_eq!(error.field, "page.per_page");
}

#[test]
fn evidence_explain_schema_declares_waterfall_contract() {
    let schema = serde_json::from_str::<Value>(EXPLAIN_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.entity.explain.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_ENTITY_EXPLAIN_WATERFALL_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["x-canon-contract"]["frozen_artifacts_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["raw_sensitive_payload_forbidden"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["deterministic_pagination_required"],
        true
    );

    let outcomes = schema["$defs"]["outcome"]["enum"]
        .as_array()
        .expect("outcome enum");
    for expected in [
        "exact_existing",
        "new_cluster",
        "linked",
        "ambiguous",
        "contradictory",
        "blocked_by_veto",
        "below_threshold",
        "review_overridden",
    ] {
        assert!(
            outcomes.iter().any(|value| value == expected),
            "schema missing outcome {expected}"
        );
    }

    let required_lanes = schema["properties"]["lanes"]["required"]
        .as_array()
        .expect("lane requirements");
    for expected in [
        "vetoes",
        "positive_score_contributions",
        "context",
        "missing_evidence",
        "policy_clauses",
        "registry_facts",
        "candidate_context",
        "solver_decisions",
        "review_overrides",
    ] {
        assert!(
            required_lanes.iter().any(|value| value == expected),
            "schema missing lane {expected}"
        );
    }
}

fn page_request(evidence_records: Vec<EvidenceExplainInputRecord>) -> EvidenceWaterfallRequest {
    EvidenceWaterfallRequest {
        decision_id: "decision:page".to_string(),
        outcome: EvidenceExplanationOutcome::Linked,
        subject_ids: vec!["subject:a".to_string(), "subject:b".to_string()],
        observed_score_units: 9000,
        threshold_score_units: 8000,
        source_artifacts: source_artifacts(),
        evidence_records,
        candidate_context: Vec::new(),
        solver_decisions: Vec::new(),
        policy_clauses: Vec::new(),
        registry_facts: Vec::new(),
        review_overrides: Vec::new(),
        missing_evidence: Vec::new(),
        redaction: EvidenceExplainRedactionPolicy::default(),
        page: EvidenceExplainPageRequest {
            page: 0,
            per_page: 3,
        },
    }
}

fn source_artifacts() -> Vec<EvidenceExplainSourceArtifact> {
    vec![
        EvidenceExplainSourceArtifact::new("evidence", "canon.evidence.v1", "blake3:evidence"),
        EvidenceExplainSourceArtifact::new("candidate", "canon.entity.block.v1", "blake3:block"),
        EvidenceExplainSourceArtifact::new("solver", "canon.entity.solve.v1", "blake3:solve"),
        EvidenceExplainSourceArtifact::new("policy", "canon.evidence.policy.v1", "blake3:policy"),
        EvidenceExplainSourceArtifact::new("registry", "canon.registry.v1", "blake3:registry"),
    ]
}
