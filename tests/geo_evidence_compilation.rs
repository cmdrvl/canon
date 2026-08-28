use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionModel, GeoCompositionStatus,
    GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoEvidenceClaimRole,
    GeoEvidenceCompilationRequest, GeoEvidenceDisposition, GeoEvidenceRecordRef, GeoIntegerMeasure,
    GeoIntegerMemberValue, GeoIntegerValueOrigin, GeoLabeledCompositionCase,
    GeoPopulationCaseStatus, GeoPopulationErrorCode, GeoPopulationEvaluationRequest, GeoRhoBasis,
    GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoRhoSoundness,
    GeoValidTimeInterval, canonical_evidence_compilation_bytes, compile_evidence,
    evaluate_population, solve_composition,
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
        source_dataset: format!("fixture:{id}"),
        source_release: "fixture-v1".to_string(),
        source_lineage_ids: vec![format!("fixture:{id}:lineage")],
        method_id: format!("fixture:{id}:rho"),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: match soundness {
            GeoRhoSoundness::LogicallySound => GeoRhoBasis::LogicalRelaxation {
                invariant_id: format!("fixture:{id}:invariant"),
            },
            GeoRhoSoundness::EmpiricalHighCoverage => GeoRhoBasis::EmpiricalCalibration {
                population_id: format!("fixture:{id}:population"),
                calibration_blake3: blake3::hash(format!("calibration:{id}").as_bytes())
                    .to_hex()
                    .to_string(),
                falsification_rule_id: format!("fixture:{id}:falsify"),
            },
        },
    }
}

fn source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-v1".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
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
                source_records: vec![source_record("exact-domain-row")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1", "p3"]), parcels(&["p1", "p2"])],
                },
            },
            GeoRhoObservation {
                id: "some-secondary".to_string(),
                contract_id: "admitted".to_string(),
                source_records: vec![source_record("some-secondary-row")],
                valid_time: None,
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
                source_records: vec![source_record("sum-band-row")],
                valid_time: None,
                observation: GeoRhoObservationKind::IntegerSumBand {
                    level: GeoEntityLevel::Parcel,
                    measure: GeoIntegerMeasure {
                        semantic_id: "fixture:computed-area".to_string(),
                        unit: "square_millimetres".to_string(),
                        value_origin: GeoIntegerValueOrigin::ExactDerived,
                    },
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
    assert!(compiled.admissions.iter().all(|admission| {
        admission.contract.soundness() == GeoRhoSoundness::LogicallySound
            && !admission.contract.source_dataset.is_empty()
            && admission.source_records.len() == 1
            && admission.source_records[0].record_blake3.len() == 64
    }));

    let solved = solve_composition(&compiled.composition_request).expect("request must solve");
    assert_eq!(solved.status, GeoCompositionStatus::Resolved);
    assert_eq!(solved.hard_forced.parcels, parcels(&["p1", "p2"]));
    assert_eq!(solved.summary.residual_model_count, 1);
}

#[test]
fn evidence_without_immutable_source_records_is_rejected_before_admission() {
    let error = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1"]),
        contracts: vec![contract("admitted", GeoRhoSoundness::LogicallySound)],
        observations: vec![GeoRhoObservation {
            id: "orphan".to_string(),
            contract_id: "admitted".to_string(),
            source_records: Vec::new(),
            valid_time: None,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "p1")],
            },
        }],
        max_assignments: 2,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect_err("an unattributable observation cannot become a solver fact");
    assert_eq!(error.code, canon::geo::GeoEvidenceErrorCode::InvalidInput);
}

#[test]
fn temporal_occupancy_cannot_be_smuggled_in_as_timeless_property_identity() {
    let mut occupancy = contract("tenant-presence", GeoRhoSoundness::LogicallySound);
    occupancy.claim_role = GeoEvidenceClaimRole::TemporalOccupancy;
    let request = |valid_time| GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2"]),
        contracts: vec![occupancy.clone()],
        observations: vec![GeoRhoObservation {
            id: "tenant-at-property".to_string(),
            contract_id: occupancy.id.clone(),
            source_records: vec![source_record("tenant-license-row")],
            valid_time,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "p1")],
            },
        }],
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let error = compile_evidence(&request(None))
        .expect_err("occupancy without valid time must not become a stable location fact");
    assert_eq!(error.code, canon::geo::GeoEvidenceErrorCode::InvalidInput);

    let interval = GeoValidTimeInterval {
        start_day: 19_723,
        end_day: 20_088,
    };
    let compiled = compile_evidence(&request(Some(interval)))
        .expect("time-bounded occupancy is admissible under its explicit role");
    assert_eq!(compiled.admissions[0].valid_time, Some(interval));
    assert_eq!(
        compiled.admissions[0].contract.claim_role,
        GeoEvidenceClaimRole::TemporalOccupancy
    );
    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    assert!(compiled.composition_request.hard_constraints.is_empty());
    assert!(compiled.composition_request.soft_preferences.is_empty());
    let solved = solve_composition(&compiled.composition_request)
        .expect("time-bounded evidence remains available without timeless pruning");
    assert_eq!(solved.summary.residual_model_count, 3);

    let mut time_scoped_anchor = request(Some(interval));
    time_scoped_anchor.contracts[0].claim_role = GeoEvidenceClaimRole::StableIdentityAnchor;
    let compiled_anchor = compile_evidence(&time_scoped_anchor)
        .expect("a time-scoped anchor remains a valid diagnostic observation");
    assert_eq!(
        compiled_anchor.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly,
        "an interval must not disappear merely because the caller labels the role stable"
    );
    assert!(
        compiled_anchor
            .composition_request
            .hard_constraints
            .is_empty()
    );
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
                source_records: vec![source_record("would-force-row")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1"])],
                },
            },
            GeoRhoObservation {
                id: "rank-p2".to_string(),
                contract_id: "calibrated-only".to_string(),
                source_records: vec![source_record("rank-p2-row")],
                valid_time: None,
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
                source_records: vec![source_record("z-only-p1-row")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p1"])],
                },
            },
            GeoRhoObservation {
                id: "a-needs-p2".to_string(),
                contract_id: "admitted".to_string(),
                source_records: vec![source_record("a-needs-p2-row")],
                valid_time: None,
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
                source_records: vec![source_record("z-soft-row")],
                valid_time: None,
                observation: GeoRhoObservationKind::PreferMember {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                    cost_if_absent: 5,
                },
            },
            GeoRhoObservation {
                id: "a-hard".to_string(),
                contract_id: "hard".to_string(),
                source_records: vec![source_record("a-hard-row")],
                valid_time: None,
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
            source_records: vec![source_record("point-row")],
            valid_time: None,
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

#[test]
fn population_keeps_candidate_reach_failures_out_of_solver_truth_metrics() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2"]),
        contracts: Vec::new(),
        observations: Vec::new(),
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "outside-candidate-reach".to_string(),
            evidence,
            truth: GeoCompositionModel {
                parcels: parcels(&["p9"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("reach failure must remain a scored population outcome");

    let case = &artifact.cases[0];
    assert!(!case.full_truth_recall);
    assert_eq!(case.truth_model_in_residual, None);
    assert!(case.backbone_complete);
    assert_eq!(artifact.summary.candidate_recall_failure_cases, 1);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 0);
    assert_eq!(artifact.summary.false_merge_cases, 0);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 0);
}

#[test]
fn population_counts_ambiguous_truth_exclusions_as_rho_contract_falsifications() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![contract("claimed-logical", GeoRhoSoundness::LogicallySound)],
        observations: vec![GeoRhoObservation {
            id: "requires-p1-or-p2".to_string(),
            contract_id: "claimed-logical".to_string(),
            source_records: vec![source_record("claimed-logical-row")],
            valid_time: None,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "p1"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                ],
            },
        }],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "falsifies-declared-contract".to_string(),
            evidence,
            truth: GeoCompositionModel {
                parcels: parcels(&["p3"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("a contract falsification is a population result");

    assert_eq!(artifact.cases[0].status, GeoPopulationCaseStatus::Ambiguous);
    assert_eq!(artifact.cases[0].truth_model_in_residual, Some(false));
    assert_eq!(artifact.summary.solver_truth_scored_cases, 1);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 1);
    assert_eq!(
        artifact.summary.false_merge_cases, 0,
        "an ambiguous exclusion is unsound but is not a false singleton merge"
    );
}

#[test]
fn population_never_reports_budget_fallback_placeholder_zero_as_a_model_count() {
    let parcel_ids = (0..12)
        .map(|index| format!("p{index:02}"))
        .collect::<Vec<_>>();
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: parcel_ids.clone(),
            buildings: Vec::new(),
        },
        contracts: vec![contract("whole-set", GeoRhoSoundness::LogicallySound)],
        observations: vec![GeoRhoObservation {
            id: "whole-set-only".to_string(),
            contract_id: "whole-set".to_string(),
            source_records: vec![source_record("whole-set-row")],
            valid_time: None,
            observation: GeoRhoObservationKind::ExactSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![parcel_ids.clone()],
            },
        }],
        max_assignments: 100,
        max_materialized_models: 0,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "bounded-search-handoff".to_string(),
            evidence,
            truth: GeoCompositionModel {
                parcels: parcel_ids,
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("fallback is a population outcome");

    let case = &artifact.cases[0];
    assert_eq!(
        case.status,
        GeoPopulationCaseStatus::ComponentBudgetFallback
    );
    assert_eq!(case.residual_model_count, None);
    assert!(!case.residual_count_saturated);
    assert_eq!(case.truth_model_in_residual, Some(true));
    assert!(!case.backbone_complete);
}

#[test]
fn population_rejects_empty_truth_labels_instead_of_calling_them_full_recall() {
    let error = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "empty-truth".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                universe: universe(&["p1"]),
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 2,
                max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
            },
            truth: GeoCompositionModel {
                parcels: Vec::new(),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect_err("empty labels cannot define evaluation truth");
    assert_eq!(error.code, GeoPopulationErrorCode::InvalidInput);
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
                    source_records: vec![source_record(&format!(
                        "{}:pip-{index:03}",
                        case.case_id
                    ))],
                    valid_time: None,
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
            case.full_truth_recall.then_some(true),
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

#[test]
fn evidence_enrichment_fixture_stays_joined_to_the_population_fixture() {
    // Companion fixture (2026-08-23): per-case asserted attributes, geocodes,
    // query address strings, and per-candidate-parcel MapPLUTO + PAD evidence
    // for the same 15 Gate V2 cases. The join contract is exact case_id
    // equality plus full candidate-parcel attribute coverage; PAD absence is
    // recorded evidence (condo billing lots, vacant lots), never an error.
    let population = population_fixture();
    let enrichment: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/geo/e4_gate_v2_evidence_enrichment.json"
    ))
    .expect("evidence enrichment fixture must parse");

    assert_eq!(
        enrichment["evidence_snapshot"]["truth_role"],
        "evaluation_only_never_solver_evidence"
    );
    assert_eq!(
        enrichment["evidence_snapshot"]["companion_of"],
        "e4_gate_v2_population.json"
    );

    let enrichment_cases = enrichment["cases"].as_array().expect("cases array");
    assert_eq!(enrichment_cases.len(), population.cases.len());
    let mut enrichment_ids: Vec<&str> = enrichment_cases
        .iter()
        .map(|case| case["case_id"].as_str().expect("case_id"))
        .collect();
    enrichment_ids.sort_unstable();
    let mut population_ids: Vec<&str> = population
        .cases
        .iter()
        .map(|case| case.case_id.as_str())
        .collect();
    population_ids.sort_unstable();
    assert_eq!(enrichment_ids, population_ids);

    let parcel_attributes = enrichment["candidate_parcel_attributes"]
        .as_object()
        .expect("candidate_parcel_attributes object");
    let mut pad_covered = 0usize;
    for case in &population.cases {
        for parcel in &case.candidate_parcels {
            assert!(
                parcel_attributes.contains_key(parcel),
                "candidate parcel {parcel} has no attribute row"
            );
        }
    }
    for attributes in parcel_attributes.values() {
        if !attributes["pad"].is_null() {
            pad_covered += 1;
        }
    }
    assert_eq!(
        pad_covered as u64,
        enrichment["expected"]["pad_covered_parcels"]
            .as_u64()
            .expect("pad_covered_parcels")
    );

    for case in enrichment_cases {
        let properties = case["properties"].as_array().expect("properties array");
        assert!(!properties.is_empty(), "case without property evidence");
        for property in properties {
            let accuracy = property["geocode"]["accuracy_type"]
                .as_str()
                .expect("accuracy_type");
            assert!(
                matches!(
                    accuracy,
                    "rooftop" | "nearest_rooftop_match" | "range_interpolation"
                ),
                "unexpected geocode accuracy tier {accuracy}"
            );
            assert!(
                !property["address_strings"]
                    .as_array()
                    .expect("addresses")
                    .is_empty()
            );
        }
    }
}
