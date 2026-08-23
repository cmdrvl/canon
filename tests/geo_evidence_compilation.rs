use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionModel, GeoCompositionStatus,
    GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoEvidenceCompilationRequest,
    GeoEvidenceDisposition, GeoIntegerMemberValue, GeoLabeledCompositionCase,
    GeoPopulationCaseStatus, GeoPopulationErrorCode, GeoPopulationEvaluationRequest,
    GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoRhoSoundness,
    canonical_evidence_compilation_bytes, compile_evidence, evaluate_population, solve_composition,
};
use serde::Deserialize;

fn parcels(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn universe(ids: &[&str]) -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: parcels(ids),
        buildings: Vec::new(),
    }
}

fn contract(id: &str, soundness: GeoRhoSoundness) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        soundness,
    }
}

#[test]
fn sound_rho_contracts_compile_exact_existential_and_integer_sum_constraints() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![contract("admitted", GeoRhoSoundness::LogicallySound)],
        observations: vec![
            GeoRhoObservation {
                id: "exact-domain".to_string(),
                contract_id: "admitted".to_string(),
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1", "p3"]), parcels(&["p1", "p2"])],
                },
            },
            GeoRhoObservation {
                id: "some-secondary".to_string(),
                contract_id: "admitted".to_string(),
                observation: GeoRhoObservationKind::ExistentialMembership {
                    members: vec![
                        GeoEntityRef::new(GeoEntityLevel::Parcel, "p3"),
                        GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                    ],
                },
            },
            GeoRhoObservation {
                id: "sum-band".to_string(),
                contract_id: "admitted".to_string(),
                observation: GeoRhoObservationKind::IntegerSumBand {
                    level: GeoEntityLevel::Parcel,
                    values: vec![
                        GeoIntegerMemberValue {
                            id: "p3".to_string(),
                            value: 30,
                        },
                        GeoIntegerMemberValue {
                            id: "p1".to_string(),
                            value: 10,
                        },
                        GeoIntegerMemberValue {
                            id: "p2".to_string(),
                            value: 20,
                        },
                    ],
                    min: 30,
                    max: 30,
                },
            },
        ],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let compiled = compile_evidence(&request).expect("sound evidence must compile");
    assert_eq!(compiled.composition_request.hard_constraints.len(), 3);
    assert!(
        compiled
            .admissions
            .iter()
            .all(|admission| admission.disposition == GeoEvidenceDisposition::HardConstraint)
    );

    let solved = solve_composition(&compiled.composition_request).expect("request must solve");
    assert_eq!(solved.status, GeoCompositionStatus::Resolved);
    assert_eq!(solved.hard_forced.parcels, parcels(&["p1", "p2"]));
    assert_eq!(solved.summary.residual_model_count, 1);
}

#[test]
fn empirical_constraints_remain_diagnostic_and_preferences_never_prune() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![contract(
            "calibrated-only",
            GeoRhoSoundness::EmpiricalHighCoverage,
        )],
        observations: vec![
            GeoRhoObservation {
                id: "would-force".to_string(),
                contract_id: "calibrated-only".to_string(),
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1"])],
                },
            },
            GeoRhoObservation {
                id: "rank-p2".to_string(),
                contract_id: "calibrated-only".to_string(),
                observation: GeoRhoObservationKind::PreferMember {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                    cost_if_absent: 7,
                },
            },
        ],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let compiled = compile_evidence(&request).expect("empirical evidence must compile safely");
    assert!(compiled.composition_request.hard_constraints.is_empty());
    assert_eq!(compiled.composition_request.soft_preferences.len(), 1);
    assert_eq!(
        compiled.admissions[1].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );

    let solved = solve_composition(&compiled.composition_request).expect("request must solve");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(solved.summary.residual_model_count, 7);
    assert!(solved.hard_forced.parcels.is_empty());
    assert!(
        solved
            .soft_ranked
            .iter()
            .take_while(|ranked| ranked.cost == 0)
            .all(|ranked| ranked.model.parcels.contains(&"p2".to_string()))
    );
}

#[test]
fn contradictory_sound_observations_name_a_deterministic_conflict() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2"]),
        contracts: vec![contract("admitted", GeoRhoSoundness::LogicallySound)],
        observations: vec![
            GeoRhoObservation {
                id: "z-only-p1".to_string(),
                contract_id: "admitted".to_string(),
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1"])],
                },
            },
            GeoRhoObservation {
                id: "a-needs-p2".to_string(),
                contract_id: "admitted".to_string(),
                observation: GeoRhoObservationKind::ExistentialMembership {
                    members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "p2")],
                },
            },
        ],
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let compiled = compile_evidence(&request).expect("sound conflict must compile");
    let solved = solve_composition(&compiled.composition_request)
        .expect("contradiction is a composition result");
    assert_eq!(solved.status, GeoCompositionStatus::Conflict);
    assert_eq!(
        solved.conflict_constraint_ids,
        ["rho:admitted@v1:a-needs-p2", "rho:admitted@v1:z-only-p1"]
    );
}

#[test]
fn evidence_compilation_is_byte_identical_under_equivalent_permutations() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![
            contract("soft", GeoRhoSoundness::EmpiricalHighCoverage),
            contract("hard", GeoRhoSoundness::LogicallySound),
        ],
        observations: vec![
            GeoRhoObservation {
                id: "z-soft".to_string(),
                contract_id: "soft".to_string(),
                observation: GeoRhoObservationKind::PreferMember {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                    cost_if_absent: 5,
                },
            },
            GeoRhoObservation {
                id: "a-hard".to_string(),
                contract_id: "hard".to_string(),
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p2", "p1"]), parcels(&["p3", "p1"])],
                },
            },
        ],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let mut permuted = request.clone();
    permuted.universe.parcels.reverse();
    permuted.contracts.reverse();
    permuted.observations.reverse();
    if let GeoRhoObservationKind::ExactSets { sets, .. } = &mut permuted.observations[1].observation
    {
        sets.reverse();
        for set in sets {
            set.reverse();
        }
    }

    let original = compile_evidence(&request).expect("original must compile");
    let permuted = compile_evidence(&permuted).expect("permutation must compile");
    assert_eq!(
        canonical_evidence_compilation_bytes(&original).expect("must serialize"),
        canonical_evidence_compilation_bytes(&permuted).expect("must serialize")
    );
}

#[test]
fn population_truth_cannot_change_compilation_or_solver_digests() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2"]),
        contracts: vec![contract(
            "observed-point",
            GeoRhoSoundness::EmpiricalHighCoverage,
        )],
        observations: vec![GeoRhoObservation {
            id: "point".to_string(),
            contract_id: "observed-point".to_string(),
            observation: GeoRhoObservationKind::PreferMember {
                member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p1"),
                cost_if_absent: 1,
            },
        }],
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let evaluate = |truth: &str| {
        evaluate_population(&GeoPopulationEvaluationRequest {
            version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
            cases: vec![GeoLabeledCompositionCase {
                id: "case".to_string(),
                evidence: evidence.clone(),
                truth: GeoCompositionModel {
                    parcels: parcels(&[truth]),
                    buildings: Vec::new(),
                },
            }],
            max_cases: 1,
        })
        .expect("population must evaluate")
    };

    let first = evaluate("p1");
    let second = evaluate("p2");
    assert_eq!(
        first.cases[0].compilation_digest,
        second.cases[0].compilation_digest
    );
    assert_eq!(first.cases[0].solver_digest, second.cases[0].solver_digest);
}

#[derive(Debug, Deserialize)]
struct PopulationFixture {
    evidence_snapshot: PopulationSnapshot,
    expected: PopulationExpected,
    cases: Vec<PopulationFixtureCase>,
}

#[derive(Debug, Deserialize)]
struct PopulationSnapshot {
    queried_on: String,
    statements: String,
    truth_role: String,
}

#[derive(Debug, Deserialize)]
struct PopulationExpected {
    cases: u64,
    truth_members: u64,
    truth_members_in_universe: u64,
    full_truth_recall_cases: u64,
    min_candidate_members: usize,
    median_candidate_members: usize,
    max_candidate_members: usize,
}

#[derive(Debug, Deserialize)]
struct PopulationFixtureCase {
    case_id: String,
    property_keys: u64,
    truth_block_count: u64,
    truth_parcels: Vec<String>,
    pip_parcels: Vec<String>,
    candidate_parcels: Vec<String>,
}

fn population_fixture() -> PopulationFixture {
    serde_json::from_str(include_str!("fixtures/geo/e4_gate_v2_population.json"))
        .expect("Gate V2 population fixture must parse")
}

#[test]
fn gate_v2_population_reports_candidate_reach_and_assignment_fallback_honestly() {
    let fixture = population_fixture();
    assert_eq!(fixture.evidence_snapshot.queried_on, "2026-08-17");
    assert_eq!(fixture.evidence_snapshot.statements, "read_only");
    assert_eq!(
        fixture.evidence_snapshot.truth_role,
        "evaluation_only_never_solver_evidence"
    );
    assert!(
        fixture
            .cases
            .iter()
            .all(|case| case.property_keys > 0 && case.truth_block_count > 0)
    );

    let mut candidate_counts = fixture
        .cases
        .iter()
        .map(|case| case.candidate_parcels.len())
        .collect::<Vec<_>>();
    candidate_counts.sort_unstable();
    assert_eq!(candidate_counts[0], fixture.expected.min_candidate_members);
    assert_eq!(
        candidate_counts[7],
        fixture.expected.median_candidate_members
    );
    assert_eq!(candidate_counts[14], fixture.expected.max_candidate_members);

    let cases = fixture
        .cases
        .into_iter()
        .map(|case| {
            let observations = case
                .pip_parcels
                .into_iter()
                .enumerate()
                .map(|(index, parcel)| GeoRhoObservation {
                    id: format!("pip-{index:03}"),
                    contract_id: "pip-point".to_string(),
                    observation: GeoRhoObservationKind::PreferMember {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, parcel),
                        cost_if_absent: 1,
                    },
                })
                .collect();
            GeoLabeledCompositionCase {
                id: case.case_id,
                evidence: GeoEvidenceCompilationRequest {
                    version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                    universe: GeoCompositionUniverse {
                        parcels: case.candidate_parcels,
                        buildings: Vec::new(),
                    },
                    contracts: vec![contract(
                        "pip-point",
                        GeoRhoSoundness::EmpiricalHighCoverage,
                    )],
                    observations,
                    max_assignments: 65_536,
                    max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
                },
                truth: GeoCompositionModel {
                    parcels: case.truth_parcels,
                    buildings: Vec::new(),
                },
            }
        })
        .collect();

    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases,
        max_cases: 15,
    })
    .expect("Gate V2 population must evaluate");

    assert_eq!(artifact.summary.cases, fixture.expected.cases);
    assert_eq!(
        artifact.summary.truth_members,
        fixture.expected.truth_members
    );
    assert_eq!(
        artifact.summary.truth_members_in_universe,
        fixture.expected.truth_members_in_universe
    );
    assert_eq!(
        artifact.summary.full_truth_recall_cases,
        fixture.expected.full_truth_recall_cases
    );
    assert_eq!(artifact.summary.resolved_cases, 0);
    // The factorized solver resolves every case that the v0 kernel could
    // only refuse on global mask budget; all residuals stay ambiguous and
    // every promotion decision remains an abstention.
    assert_eq!(artifact.summary.ambiguous_cases, 15);
    assert_eq!(artifact.summary.assignment_budget_exceeded_cases, 0);
    assert_eq!(artifact.summary.component_budget_fallback_cases, 0);
    assert_eq!(artifact.summary.abstention_cases, 15);
    assert_eq!(artifact.summary.false_merge_cases, 0);
    assert_eq!(artifact.summary.backbone_false_positive_members, 0);
    assert!(
        artifact
            .cases
            .iter()
            .all(|case| case.status == GeoPopulationCaseStatus::Ambiguous)
    );
    // Saturation honesty: universes whose free-variable residual exceeds the
    // u64 reporting range declare a lower bound instead of a fake count.
    let fixture_by_id = |id: &str| {
        population_fixture()
            .cases
            .into_iter()
            .find(|case| case.case_id == id)
            .expect("fixture case must exist")
    };
    for case in &artifact.cases {
        let source = fixture_by_id(&case.case_id);
        assert_eq!(
            case.residual_count_saturated,
            source.candidate_parcels.len() >= 64,
            "case {} saturation must match its candidate width",
            case.case_id
        );
        assert_eq!(
            case.truth_model_in_residual,
            Some(case.full_truth_recall),
            "truth models inside the universe must survive in the residual"
        );
    }
}

#[test]
fn population_case_budget_refuses_before_any_case_runs() {
    let error = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: Vec::new(),
        max_cases: 0,
    })
    .expect_err("zero population budget must refuse");
    assert_eq!(error.code, GeoPopulationErrorCode::PopulationBudgetExceeded);
}
