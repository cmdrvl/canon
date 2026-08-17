use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, GeoBuildingCandidate, GeoCompositionErrorCode,
    GeoCompositionRequest, GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel,
    GeoEntityRef, GeoHardConstraint, GeoHardConstraintKind, GeoIdentityRelation,
    canonical_composition_bytes, solve_composition, validate_identity_relation,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    evidence_snapshot: EvidenceSnapshot,
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct EvidenceSnapshot {
    queried_on: String,
    statements: String,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
    expected: ExpectedOutcome,
}

#[derive(Debug, Deserialize)]
struct ExpectedOutcome {
    status: GeoCompositionStatus,
    residual_model_count: u64,
    hard_forced_parcels: Vec<String>,
    hard_forced_buildings: Vec<String>,
    residual_building_sets: Vec<Vec<String>>,
}

fn corpus() -> WorkedCorpus {
    serde_json::from_str(include_str!("fixtures/geo/e4_worked_cases.json"))
        .expect("E4 worked-case fixture must parse")
}

#[test]
fn all_six_worked_case_shapes_produce_the_declared_entity_grain_residual() {
    let corpus = corpus();
    assert_eq!(corpus.evidence_snapshot.queried_on, "2026-08-17");
    assert_eq!(corpus.evidence_snapshot.statements, "read_only");
    assert_eq!(corpus.cases.len(), 6);

    for case in corpus.cases {
        let artifact = solve_composition(&case.request)
            .unwrap_or_else(|error| panic!("{} failed: {error}", case.case_id));
        assert_eq!(artifact.status, case.expected.status, "{}", case.case_id);
        assert_eq!(
            artifact.summary.residual_model_count, case.expected.residual_model_count,
            "{}",
            case.case_id
        );
        assert_eq!(
            artifact.hard_forced.parcels, case.expected.hard_forced_parcels,
            "{}",
            case.case_id
        );
        assert_eq!(
            artifact.hard_forced.buildings, case.expected.hard_forced_buildings,
            "{}",
            case.case_id
        );
        assert_eq!(
            artifact
                .residual_models
                .iter()
                .map(|model| model.buildings.clone())
                .collect::<Vec<_>>(),
            case.expected.residual_building_sets,
            "{}",
            case.case_id
        );
    }
}

#[test]
fn case_six_soft_point_ranks_without_forcing_a_building() {
    let case = corpus()
        .cases
        .into_iter()
        .find(|case| case.case_id == "case_6_dense_one_parcel_multi_building")
        .expect("case 6 must exist");
    let artifact = solve_composition(&case.request).expect("case 6 must solve");

    assert_eq!(artifact.status, GeoCompositionStatus::Ambiguous);
    assert!(artifact.hard_forced.buildings.is_empty());
    assert_eq!(artifact.residual_models.len(), 2);
    assert_eq!(artifact.soft_ranked[0].model.buildings, ["1076314"]);
    assert_eq!(artifact.soft_ranked[0].cost, 0);
    assert_eq!(artifact.soft_ranked[1].model.buildings, ["1085187"]);
    assert_eq!(artifact.soft_ranked[1].cost, 10);
}

#[test]
fn cross_level_same_as_is_rejected_but_typed_containment_is_allowed() {
    let building = GeoEntityRef::new(GeoEntityLevel::Building, "building-a");
    let parcel = GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a");

    let error = validate_identity_relation(&building, &parcel, GeoIdentityRelation::SameAs)
        .expect_err("cross-level same_as must fail");
    assert_eq!(error.code, GeoCompositionErrorCode::InvalidInput);
    assert!(error.message.contains("Cross-level"));
    validate_identity_relation(&building, &parcel, GeoIdentityRelation::On)
        .expect("typed containment must remain expressible");
}

#[test]
fn contradictory_constraints_return_a_deterministic_irreducible_conflict() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: vec!["parcel-a".to_string(), "parcel-b".to_string()],
            buildings: Vec::new(),
        },
        hard_constraints: vec![
            GeoHardConstraint {
                id: "z_only_a".to_string(),
                constraint: GeoHardConstraintKind::AllowedSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["parcel-a".to_string()]],
                },
            },
            GeoHardConstraint {
                id: "a_forbid".to_string(),
                constraint: GeoHardConstraintKind::Forbid {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                },
            },
        ],
        soft_preferences: Vec::new(),
        max_assignments: 4,
    };

    let artifact = solve_composition(&request).expect("conflict is a domain result");
    assert_eq!(artifact.status, GeoCompositionStatus::Conflict);
    assert!(artifact.residual_models.is_empty());
    assert_eq!(artifact.conflict_constraint_ids, ["a_forbid", "z_only_a"]);
}

#[test]
fn assignment_budget_refuses_before_enumeration() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: (0..10).map(|index| format!("parcel-{index:02}")).collect(),
            buildings: Vec::new(),
        },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 100,
    };

    let error = solve_composition(&request).expect_err("2^10 exceeds the declared budget");
    assert_eq!(error.code, GeoCompositionErrorCode::BudgetExceeded);
    assert_eq!(error.detail["estimated_assignments"], "1024");
    assert_eq!(error.detail["max_assignments"], "100");
}

#[test]
fn equivalent_input_permutations_serialize_to_identical_bytes() {
    let case = corpus()
        .cases
        .into_iter()
        .find(|case| case.case_id == "case_4_chimera_multi_street")
        .expect("case 4 must exist");
    let mut permuted = case.request.clone();
    permuted.universe.parcels.reverse();
    permuted.universe.buildings.reverse();
    for building in &mut permuted.universe.buildings {
        building.parcel_ids.reverse();
    }
    permuted.hard_constraints.reverse();
    for constraint in &mut permuted.hard_constraints {
        if let GeoHardConstraintKind::AllowedSets { sets, .. } = &mut constraint.constraint {
            sets.reverse();
            for set in sets {
                set.reverse();
            }
        }
    }
    permuted.soft_preferences.reverse();

    let original = solve_composition(&case.request).expect("original must solve");
    let permuted = solve_composition(&permuted).expect("permutation must solve");
    assert_eq!(
        canonical_composition_bytes(&original).expect("artifact must serialize"),
        canonical_composition_bytes(&permuted).expect("artifact must serialize")
    );
}

#[test]
fn selected_building_requires_one_of_its_declared_parcels() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: vec!["parcel-a".to_string(), "parcel-b".to_string()],
            buildings: vec![GeoBuildingCandidate {
                id: "building-a".to_string(),
                parcel_ids: vec!["parcel-a".to_string()],
            }],
        },
        hard_constraints: vec![
            GeoHardConstraint {
                id: "select_other_parcel".to_string(),
                constraint: GeoHardConstraintKind::AllowedSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["parcel-b".to_string()]],
                },
            },
            GeoHardConstraint {
                id: "select_building".to_string(),
                constraint: GeoHardConstraintKind::Require {
                    member: GeoEntityRef::new(GeoEntityLevel::Building, "building-a"),
                },
            },
        ],
        soft_preferences: Vec::new(),
        max_assignments: 8,
    };

    let artifact = solve_composition(&request).expect("structural conflict must report");
    assert_eq!(artifact.status, GeoCompositionStatus::Conflict);
    assert!(artifact.residual_models.is_empty());
}
