#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_COMPOSITION_VERSION,
    CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION, CANON_GEO_NEXT_EVIDENCE_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_SEPARATION_VERSION, GeoBudgetAction,
    GeoCompositionArtifact, GeoCompositionBackbone, GeoCompositionProfile, GeoCompositionStatus,
    GeoCompositionSummary, GeoDecisionPolicyRef, GeoDominanceBasis, GeoLossModelRef,
    GeoModelCountScope, GeoNextAction, GeoNextActionClass, GeoNextActionKind,
    GeoNextEvidencePolicy, GeoNextEvidenceRequest, GeoNumericBound, GeoObservationSeparation,
    GeoOutcomeSeparation, GeoResourceBudget, GeoResourceCounter, GeoSeparationArtifact,
    GeoStopReason, GeoValueOrigin, canonical_composition_bytes, canonical_next_evidence_bytes,
    canonical_next_evidence_request_bytes, canonical_separation_bytes, recommend,
    recommend_from_request,
};
use serde_json::Value;
use std::{collections::BTreeMap, fs};

const CASE_444: &str =
    "h7-subject:non-round:c8217df560eb3d08527d5f562299988dd2e287326ca61590a0c9f462bf1ab251";
const CASE_312: &str =
    "h7-subject:non-round:ae5fa9ee31b1670d1af89f8fd3632c4f207957a15486b4007a6776495f960309";
const D1_FIXTURE_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03";

#[test]
fn t18_frontier_without_loss_model_and_total_ranking_with_loss_model() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![
            observation(
                "A",
                outcomes(&[("left", 2, true), ("right", 2, true)]),
                false,
            ),
            observation(
                "B",
                outcomes(&[("left", 2, true), ("right", 2, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "A",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 2, true), ("right", 2, true)]),
            false,
        ),
        action(
            "B",
            GeoNextActionClass::SeparateResidual,
            2,
            outcomes(&[("left", 2, true), ("right", 2, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("T18 recommendation should build");
    log_artifact("T18/no-policy", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A"]);
    assert_eq!(ids(&artifact.dominated), ["B"]);
    assert_eq!(artifact.dominated[0].dominated_by, ["A"]);
    assert!(artifact.total_ranking.is_none());
    assert!(artifact.stop.is_none());
    assert_eq!(
        artifact
            .ranking_abstention
            .as_ref()
            .and_then(|abstention| { abstention.detail.get("policy").map(String::as_str) }),
        Some("none")
    );

    let policy = policy_with_loss_model();
    let ranked = recommend(
        &composition,
        &separation,
        &candidates,
        Some(&policy),
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("T18 recommendation with loss model should build");
    log_artifact("T18/with-loss-model", &candidates, Some(&policy), &ranked);
    assert_eq!(
        ranked.total_ranking,
        Some(vec!["A".to_string(), "B".to_string()])
    );
    assert!(ranked.ranking_abstention.is_none());
}

#[test]
fn negative_tradeoff_candidates_both_remain_frontier_without_policy_ranking() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![
            observation(
                "C",
                outcomes(&[("left", 3, true), ("right", 3, true)]),
                false,
            ),
            observation(
                "D",
                outcomes(&[("left", 1, true), ("right", 1, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "C",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 3, true), ("right", 3, true)]),
            false,
        ),
        action(
            "D",
            GeoNextActionClass::SeparateResidual,
            3,
            outcomes(&[("left", 1, true), ("right", 1, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("tradeoff recommendation should build");
    log_artifact("tradeoff", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["C", "D"]);
    assert!(artifact.dominated.is_empty());
    assert!(artifact.total_ranking.is_none());
}

#[test]
fn negative_hard_forced_claim_stops_with_empty_frontier() {
    let composition = composition(GeoCompositionStatus::Resolved, 1, true);
    let separation = separation(
        1,
        vec![observation("A", outcomes(&[("forced", 1, true)]), false)],
    );
    let candidates = vec![action(
        "A",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("forced", 1, true)]),
        false,
    )];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("forced composition should stop");
    log_artifact("forced", &candidates, None, &artifact);

    assert!(artifact.frontier.is_empty());
    assert!(artifact.dominated.is_empty());
    assert_eq!(artifact.stop, Some(GeoStopReason::ClaimForced));
}

#[test]
fn negative_inexact_separation_has_bounds_and_no_information_value_keys() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![
            observation(
                "A",
                outcomes(&[("left", 2, false), ("right", 2, false)]),
                false,
            ),
            observation(
                "B",
                outcomes(&[("left", 3, false), ("right", 3, false)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "A",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 2, false), ("right", 2, false)]),
            false,
        ),
        action(
            "B",
            GeoNextActionClass::SeparateResidual,
            2,
            outcomes(&[("left", 3, false), ("right", 3, false)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("inexact recommendation should build");
    log_artifact("inexact", &candidates, None, &artifact);

    assert!(
        artifact
            .frontier
            .iter()
            .flat_map(|action| action.separation.iter())
            .all(|outcome| !outcome.count_exact)
    );
    assert!(
        artifact
            .dominance_basis
            .values()
            .all(|basis| *basis == GeoDominanceBasis::Bounds)
    );
    let value = serde_json::to_value(&artifact).expect("artifact serializes");
    let keys = collect_keys(&value);
    eprintln!("negative field keys: {keys:?}");
    for forbidden in ["expect", "probab", "voi", "likelihood", "gain"] {
        assert!(
            keys.iter()
                .all(|key| !key.to_ascii_lowercase().contains(forbidden)),
            "forbidden key fragment {forbidden} in {keys:?}"
        );
    }
}

#[test]
fn negative_redundant_and_shared_lineage_never_reaches_frontier() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![
            observation(
                "A",
                outcomes(&[("left", 2, true), ("right", 2, true)]),
                false,
            ),
            observation(
                "B",
                outcomes(&[("left", 2, true), ("right", 2, true)]),
                true,
            ),
            observation(
                "C",
                outcomes(&[("left", 2, true), ("right", 2, true)]),
                false,
            ),
        ],
    );
    let mut a = action(
        "A",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("left", 2, true), ("right", 2, true)]),
        false,
    );
    a.lineage_ids = vec!["lineage.shared.fixture".to_string()];
    let mut b = action(
        "B",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("left", 2, true), ("right", 2, true)]),
        false,
    );
    b.lineage_ids = vec!["lineage.unique.fixture".to_string()];
    let mut c = action(
        "C",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("left", 2, true), ("right", 2, true)]),
        false,
    );
    c.lineage_ids = vec!["lineage.shared.fixture".to_string()];
    let candidates = vec![a, b, c];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("redundant recommendation should build");
    log_artifact("redundant", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A"]);
    assert_eq!(ids(&artifact.dominated), ["B", "C"]);
    assert_eq!(artifact.dominated[0].action_id, "B");
    assert!(
        artifact.dominated[1]
            .dominated_by
            .contains(&"A".to_string()),
        "shared-lineage duplicate should be tied to the first prospective observation"
    );
}

#[test]
fn negative_operations_budget_exhaustion_stops_but_keeps_frontier() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![observation(
            "A",
            outcomes(&[("left", 2, true), ("right", 2, true)]),
            false,
        )],
    );
    let candidates = vec![action(
        "A",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("left", 2, true), ("right", 2, true)]),
        false,
    )];
    let mut spent = BTreeMap::new();
    spent.insert("ops".to_string(), 10);

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &spent,
    )
    .expect("budget-exhausted recommendation should build");
    log_artifact("budget", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A"]);
    assert_eq!(artifact.stop, Some(GeoStopReason::BudgetExceeded));
    assert_eq!(artifact.budget_remaining.get("ops"), Some(&0));
}

#[test]
fn negative_inexact_counts_do_not_dominate_exact_equal_cost() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![
            observation(
                "A",
                outcomes(&[("left", 2, false), ("right", 2, false)]),
                false,
            ),
            observation(
                "B",
                outcomes(&[("left", 3, true), ("right", 3, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "A",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 2, false), ("right", 2, false)]),
            false,
        ),
        action(
            "B",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 3, true), ("right", 3, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("mixed exactness recommendation should build");
    log_artifact("mixed-exactness", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A", "B"]);
    assert!(artifact.dominated.is_empty());
    assert_eq!(
        artifact.dominance_basis.get("A>B"),
        Some(&GeoDominanceBasis::Bounds)
    );
}

#[test]
fn reach_failure_repair_precedes_residual_separation() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 8, false);
    let separation = separation(
        8,
        vec![
            observation(
                "A",
                outcomes(&[("absent", 4, true), ("added", 4, true)]),
                false,
            ),
            observation(
                "B",
                outcomes(&[("left", 1, true), ("right", 1, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "A",
            GeoNextActionClass::RepairReach,
            5,
            outcomes(&[("absent", 4, true), ("added", 4, true)]),
            false,
        ),
        action(
            "B",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("left", 1, true), ("right", 1, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("reach repair recommendation should build");
    log_artifact("reach", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A"]);
}

#[test]
fn empty_residual_conflict_diagnosis_precedes_unrelated_sources() {
    let composition = composition(GeoCompositionStatus::Conflict, 0, false);
    let separation = separation(
        0,
        vec![
            observation("A", outcomes(&[("core", 0, true)]), false),
            observation("B", outcomes(&[("other", 0, true)]), false),
        ],
    );
    let candidates = vec![
        action(
            "A",
            GeoNextActionClass::DiagnoseConflict,
            1,
            outcomes(&[("core", 0, true)]),
            false,
        ),
        action(
            "B",
            GeoNextActionClass::RaiseClaimClass,
            1,
            outcomes(&[("other", 0, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("conflict diagnosis recommendation should build");
    log_artifact("conflict", &candidates, None, &artifact);

    assert_eq!(ids(&artifact.frontier), ["A"]);
    assert!(artifact.stop.is_none());
}

#[test]
fn request_and_artifact_schema_instances_canonicalize() {
    let composition = composition(GeoCompositionStatus::Ambiguous, 4, false);
    let separation = separation(
        4,
        vec![observation(
            "A",
            outcomes(&[("left", 2, true), ("right", 2, true)]),
            false,
        )],
    );
    let candidates = vec![action(
        "A",
        GeoNextActionClass::SeparateResidual,
        1,
        outcomes(&[("left", 2, true), ("right", 2, true)]),
        false,
    )];
    let request = GeoNextEvidenceRequest {
        version: CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION.to_string(),
        composition_blake3: prefixed_hash(&canonical_composition_bytes(&composition).unwrap()),
        separation_blake3: prefixed_hash(&canonical_separation_bytes(&separation).unwrap()),
        candidates: candidates.clone(),
        policy: None,
        budget: budget(10),
        budget_spent: BTreeMap::new(),
    };
    let request_bytes =
        canonical_next_evidence_request_bytes(&request).expect("request canonicalizes");
    let request_instance: Value = serde_json::from_slice(&request_bytes).unwrap();
    assert_top_level_keys_declared(
        include_str!("../schemas/canon.geo.next_evidence_request.v0.schema.json"),
        &request_instance,
    );

    let artifact =
        recommend_from_request(&composition, &separation, &request).expect("request recommends");
    let artifact_bytes = canonical_next_evidence_bytes(&artifact).expect("artifact canonicalizes");
    let artifact_instance: Value = serde_json::from_slice(&artifact_bytes).unwrap();
    assert_top_level_keys_declared(
        include_str!("../schemas/canon.geo.next_evidence.v0.schema.json"),
        &artifact_instance,
    );
    assert_eq!(
        request_instance["version"],
        CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION
    );
    assert_eq!(
        artifact_instance["version"],
        CANON_GEO_NEXT_EVIDENCE_VERSION
    );
}

#[test]
fn d1_fixture_444_86th_street_selects_separate_residual_action() {
    let before = d1_case("evaluation_roll_universe_owner.json", CASE_444);
    let after = d1_case("evaluation_roll_exact_owner_gsf_band.json", CASE_444);
    eprintln!("proof_class=fixture fixture_case=444_86th_street before={before:?} after={after:?}");
    assert_eq!(before["status"], "ambiguous");
    assert_eq!(before["residual_model_count"], 16);
    assert_eq!(after["status"], "resolved");
    assert_eq!(after["residual_model_count"], 1);

    let composition = composition(GeoCompositionStatus::Ambiguous, 16, false);
    let separation = separation(
        16,
        vec![
            observation(
                "fixture.444_86th_street.roll_gsf_band",
                outcomes(&[("out_of_band", 16, true), ("within_band", 1, true)]),
                false,
            ),
            observation(
                "fixture.444_86th_street.roll_owner",
                outcomes(&[("keeps_current", 16, true), ("narrows", 16, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "fixture.444_86th_street.roll_gsf_band",
            GeoNextActionClass::SeparateResidual,
            1,
            outcomes(&[("out_of_band", 16, true), ("within_band", 1, true)]),
            false,
        ),
        action(
            "fixture.444_86th_street.roll_owner",
            GeoNextActionClass::SeparateResidual,
            2,
            outcomes(&[("keeps_current", 16, true), ("narrows", 16, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("444 fixture recommendation should build");
    log_artifact("d1/444", &candidates, None, &artifact);
    assert_eq!(
        ids(&artifact.frontier),
        ["fixture.444_86th_street.roll_gsf_band"]
    );
    assert!(artifact.stop.is_none());
}

#[test]
fn d1_fixture_312_97th_street_selects_conflict_diagnosis_action() {
    let receipt = d1_case("evaluation_roll_exact_owner_gsf_band.json", CASE_312);
    eprintln!("proof_class=fixture fixture_case=312_97th_street receipt={receipt:?}");
    assert_eq!(receipt["status"], "conflict");
    assert_eq!(receipt["residual_model_count"], 0);

    let composition = composition(GeoCompositionStatus::Conflict, 0, false);
    let separation = separation(
        0,
        vec![
            observation(
                "fixture.312_97th_street.conflict_core",
                outcomes(&[("core_minimal", 0, true)]),
                false,
            ),
            observation(
                "fixture.312_97th_street.unrelated_source",
                outcomes(&[("unrelated", 0, true)]),
                false,
            ),
        ],
    );
    let candidates = vec![
        action(
            "fixture.312_97th_street.conflict_core",
            GeoNextActionClass::DiagnoseConflict,
            1,
            outcomes(&[("core_minimal", 0, true)]),
            false,
        ),
        action(
            "fixture.312_97th_street.unrelated_source",
            GeoNextActionClass::RaiseClaimClass,
            1,
            outcomes(&[("unrelated", 0, true)]),
            false,
        ),
    ];

    let artifact = recommend(
        &composition,
        &separation,
        &candidates,
        None,
        &budget(10),
        &BTreeMap::new(),
    )
    .expect("312 fixture recommendation should build");
    log_artifact("d1/312", &candidates, None, &artifact);
    assert_eq!(
        ids(&artifact.frontier),
        ["fixture.312_97th_street.conflict_core"]
    );
    assert!(artifact.stop.is_none());
}

fn composition(
    status: GeoCompositionStatus,
    residual_model_count: u64,
    forced: bool,
) -> GeoCompositionArtifact {
    GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        evidence_compilation: None,
        status,
        resolved_claim: None,
        summary: GeoCompositionSummary {
            parcel_candidates: residual_model_count.max(1) as usize,
            building_candidates: 0,
            candidate_assignments: residual_model_count.max(1),
            candidate_assignments_saturated: false,
            structurally_feasible_assignments: residual_model_count.max(1),
            structurally_feasible_assignments_complete: true,
            structurally_feasible_assignments_saturated: false,
            hard_constraint_evaluations: 0,
            hard_constraint_evaluations_complete: true,
            hard_constraint_evaluations_saturated: false,
            residual_model_count,
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: true,
            residual_model_count_saturated: false,
            summary_counts_saturated: false,
            component_count: residual_model_count.max(1) as usize,
            residual_models_materialized: false,
        },
        hard_forced: GeoCompositionBackbone {
            parcels: if forced {
                vec!["forced-parcel".to_string()]
            } else {
                Vec::new()
            },
            buildings: Vec::new(),
        },
        backbone_complete: true,
        factorization: Vec::new(),
        residual_models: Vec::new(),
        soft_ranked: Vec::new(),
        conflict_constraint_ids: if status == GeoCompositionStatus::Conflict {
            vec!["conflict.constraint".to_string()]
        } else {
            Vec::new()
        },
        conflict_core_complete: (status == GeoCompositionStatus::Conflict).then_some(true),
        budget_fallback: None,
        entity_projection: None,
    }
}

fn separation(
    baseline_model_count: u64,
    per_observation: Vec<GeoObservationSeparation>,
) -> GeoSeparationArtifact {
    GeoSeparationArtifact {
        version: CANON_GEO_SEPARATION_VERSION.to_string(),
        subject_ref: None,
        request_blake3: "blake3:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        baseline_model_count,
        per_observation,
    }
}

fn observation(
    observation_id: &str,
    per_outcome: Vec<GeoOutcomeSeparation>,
    redundant: bool,
) -> GeoObservationSeparation {
    GeoObservationSeparation {
        observation_id: observation_id.to_string(),
        worst_case_remaining: per_outcome
            .iter()
            .map(|outcome| outcome.residual_model_count)
            .max()
            .unwrap_or_default(),
        per_outcome,
        redundant,
    }
}

fn outcomes(values: &[(&str, u64, bool)]) -> Vec<GeoOutcomeSeparation> {
    values
        .iter()
        .map(
            |(outcome_id, residual_model_count, count_exact)| GeoOutcomeSeparation {
                outcome_id: (*outcome_id).to_string(),
                residual_model_count: *residual_model_count,
                count_exact: *count_exact,
            },
        )
        .collect()
}

fn action(
    action_id: &str,
    class: GeoNextActionClass,
    cost_units: u64,
    separation: Vec<GeoOutcomeSeparation>,
    redundant: bool,
) -> GeoNextAction {
    let worst_case_remaining = separation
        .iter()
        .map(|outcome| outcome.residual_model_count)
        .max()
        .unwrap_or_default();
    GeoNextAction {
        action_id: action_id.to_string(),
        class,
        kind: GeoNextActionKind::Observe(action_id.to_string()),
        observation_id: Some(action_id.to_string()),
        cost_units,
        separation,
        worst_case_remaining,
        redundant,
        lineage_ids: Vec::new(),
        dominated_by: Vec::new(),
        stop_reason: None,
    }
}

fn budget(operations: u64) -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.next_evidence".to_string(),
        deterministic_bounds: vec![GeoNumericBound {
            semantic_id: "ops".to_string(),
            counter: GeoResourceCounter::Operations,
            value: operations,
            unit: "operation".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::ReportBudgetFallback,
        }],
        telemetry: Vec::new(),
    }
}

fn policy_with_loss_model() -> GeoNextEvidencePolicy {
    GeoNextEvidencePolicy {
        policy: GeoDecisionPolicyRef {
            policy_id: "policy.fixture.total-ranking".to_string(),
            version: "1.0.0".to_string(),
            content_hash: "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
        },
        loss_model: Some(GeoLossModelRef {
            loss_model_id: "loss.fixture.counts".to_string(),
            version: "1.0.0".to_string(),
            content_hash: "blake3:2222222222222222222222222222222222222222222222222222222222222222"
                .to_string(),
        }),
    }
}

fn ids<const N: usize>(actions: &[GeoNextAction]) -> [&str; N] {
    actions
        .iter()
        .map(|action| action.action_id.as_str())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or_else(|ids: Vec<&str>| panic!("unexpected ids: {ids:?}"))
}

fn collect_keys(value: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    collect_keys_into(value, &mut keys);
    keys.sort();
    keys.dedup();
    keys
}

fn collect_keys_into(value: &Value, keys: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                keys.push(key.clone());
                collect_keys_into(nested, keys);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_keys_into(nested, keys);
            }
        }
        _ => {}
    }
}

fn assert_top_level_keys_declared(schema: &str, instance: &Value) {
    let schema: Value = serde_json::from_str(schema).expect("schema parses");
    let properties = schema["properties"]
        .as_object()
        .expect("schema has top-level properties");
    let object = instance.as_object().expect("instance is object");
    for key in object.keys() {
        assert!(
            properties.contains_key(key),
            "{key} was not declared in next-evidence schema top-level properties"
        );
    }
}

fn d1_case(file_name: &str, case_id: &str) -> Value {
    let path = format!("{D1_FIXTURE_DIR}/{file_name}");
    let bytes = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let value: Value =
        serde_json::from_str(&bytes).unwrap_or_else(|error| panic!("parse {path}: {error}"));
    value["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .find(|case| case["case_id"].as_str() == Some(case_id))
        })
        .cloned()
        .unwrap_or_else(|| panic!("fixture case {case_id} not found in {path}"))
}

fn log_artifact(
    label: &str,
    candidates: &[GeoNextAction],
    policy: Option<&GeoNextEvidencePolicy>,
    artifact: &canon::geo::GeoNextEvidenceArtifact,
) {
    eprintln!(
        "{label} candidates={:?} policy={:?} frontier={:?} dominated={:?} total_ranking={:?} stop={:?} dominance_basis={:?}",
        candidates
            .iter()
            .map(|candidate| (
                candidate.action_id.as_str(),
                candidate.class,
                candidate.cost_units,
                candidate.worst_case_remaining,
                candidate
                    .separation
                    .iter()
                    .map(|outcome| (
                        outcome.outcome_id.as_str(),
                        outcome.residual_model_count,
                        outcome.count_exact
                    ))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        policy.map(|policy| (
            policy.policy.policy_id.as_str(),
            policy.loss_model.is_some()
        )),
        ids_vec(&artifact.frontier),
        artifact
            .dominated
            .iter()
            .map(|action| (
                action.action_id.as_str(),
                action
                    .dominated_by
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        artifact.total_ranking,
        artifact.stop,
        artifact.dominance_basis
    );
}

fn ids_vec(actions: &[GeoNextAction]) -> Vec<&str> {
    actions
        .iter()
        .map(|action| action.action_id.as_str())
        .collect()
}

fn prefixed_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
