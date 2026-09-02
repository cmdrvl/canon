use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCandidateReachStatus,
    GeoCompositionModel, GeoCompositionProfile, GeoCompositionStatus, GeoCompositionUniverse,
    GeoEntityLevel, GeoEntityRef, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest,
    GeoEvidenceCoverageStatus, GeoEvidenceDisposition, GeoEvidenceRecordRef, GeoIntegerMeasure,
    GeoIntegerMemberValue, GeoIntegerValueOrigin, GeoLabeledCompositionCase,
    GeoPopulationCaseStatus, GeoPopulationErrorCode, GeoPopulationEvaluationArtifact,
    GeoPopulationEvaluationRequest, GeoPopulationSummary, GeoPopulationTruthPlaneSummary,
    GeoRhoBasis, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoRhoSoundness,
    GeoTruthPlane, GeoValidTimeInterval, canonical_evidence_compilation_bytes, compile_evidence,
    evaluate_population, solve_composition, validate_population_evaluation_artifact,
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
                admissible_hard_band: false,
            },
        },
    }
}

fn flagged_empirical_contract(id: &str) -> GeoRhoContract {
    let mut contract = contract(id, GeoRhoSoundness::EmpiricalHighCoverage);
    if let GeoRhoBasis::EmpiricalCalibration {
        admissible_hard_band,
        ..
    } = &mut contract.basis
    {
        *admissible_hard_band = true;
    }
    contract
}

fn integer_sum_observation(id: &str, contract_id: &str) -> GeoRhoObservation {
    GeoRhoObservation {
        id: id.to_string(),
        contract_id: contract_id.to_string(),
        source_records: vec![source_record(&format!("{id}-row"))],
        valid_time: None,
        observation: GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: "fixture:structure-count".to_string(),
                unit: "count".to_string(),
                value_origin: GeoIntegerValueOrigin::ExactDerived,
            },
            values: vec![
                GeoIntegerMemberValue {
                    id: "p1".to_string(),
                    value: 1,
                },
                GeoIntegerMemberValue {
                    id: "p2".to_string(),
                    value: 1,
                },
                GeoIntegerMemberValue {
                    id: "p3".to_string(),
                    value: 1,
                },
            ],
            min: 2,
            max: 2,
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

fn building_universe(ids: &[&str]) -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: Vec::new(),
        buildings: ids
            .iter()
            .map(|id| GeoBuildingCandidate {
                id: (*id).to_string(),
                parcel_ids: Vec::new(),
            })
            .collect(),
    }
}

fn hard_population_case(
    id: &str,
    plane: GeoTruthPlane,
    universe_ids: &[&str],
    forced: &str,
    truth_ids: &[&str],
) -> GeoLabeledCompositionCase {
    GeoLabeledCompositionCase {
        id: id.to_string(),
        evidence: GeoEvidenceCompilationRequest {
            version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
            profile: Default::default(),
            universe: universe(universe_ids),
            contracts: vec![contract(id, GeoRhoSoundness::LogicallySound)],
            observations: vec![GeoRhoObservation {
                id: "declared-hard-evidence".to_string(),
                contract_id: id.to_string(),
                source_records: vec![source_record(&format!("{id}-row"))],
                valid_time: None,
                observation: GeoRhoObservationKind::ExistentialMembership {
                    members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, forced)],
                },
            }],
            max_assignments: 8,
            max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
        },
        truth_plane: plane,
        truth: GeoCompositionModel {
            parcels: parcels(truth_ids),
            buildings: Vec::new(),
        },
    }
}

fn mixed_denominator_population() -> GeoPopulationEvaluationArtifact {
    evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![
            hard_population_case(
                "gate-none-reach",
                GeoTruthPlane::GateV2Historical,
                &["p1"],
                "p1",
                &["p9"],
            ),
            hard_population_case(
                "gate-partial-reach",
                GeoTruthPlane::GateV2Historical,
                &["p1", "p2"],
                "p1",
                &["p1", "p9"],
            ),
            hard_population_case(
                "round-full-falsification",
                GeoTruthPlane::RoundExactLenderParty,
                &["p1", "p2", "p3"],
                "p1",
                &["p3"],
            ),
        ],
        max_cases: 3,
    })
    .expect("mixed population evaluates")
}

fn plane_sum(
    summary: &GeoPopulationSummary,
    field: impl Fn(&GeoPopulationTruthPlaneSummary) -> u64,
) -> u64 {
    summary.truth_planes.iter().map(field).sum()
}

fn assert_summary_matches_truth_plane_sums(summary: &GeoPopulationSummary) {
    macro_rules! assert_plane_sum {
        ($field:ident) => {
            assert_eq!(
                summary.$field,
                plane_sum(summary, |plane| plane.$field),
                "{}",
                stringify!($field)
            );
        };
    }

    assert_plane_sum!(cases);
    assert_plane_sum!(population_eligible_cases);
    assert_plane_sum!(resolved_cases);
    assert_plane_sum!(ambiguous_cases);
    assert_plane_sum!(conflict_cases);
    assert_plane_sum!(abstention_cases);
    assert_plane_sum!(false_merge_cases);
    assert_plane_sum!(candidate_reach_evaluated_cases);
    assert_plane_sum!(candidate_reach_full_cases);
    assert_plane_sum!(candidate_reach_partial_cases);
    assert_plane_sum!(candidate_reach_none_cases);
    assert_plane_sum!(solver_truth_scored_cases);
    assert_plane_sum!(solver_artifact_cases);
    assert_plane_sum!(empirical_falsification_eligible_cases);
    assert_plane_sum!(solver_truth_exclusion_cases);
    assert_plane_sum!(residual_count_complete_cases);
    assert_plane_sum!(residual_count_saturated_cases);
    assert_plane_sum!(residual_count_unavailable_cases);
    assert_plane_sum!(component_budget_fallback_cases);
    assert_plane_sum!(assignment_budget_exceeded_cases);
    assert_plane_sum!(evidence_no_observation_cases);
    assert_plane_sum!(evidence_diagnostic_only_cases);
    assert_plane_sum!(evidence_soft_preference_only_cases);
    assert_plane_sum!(evidence_soft_and_diagnostic_only_cases);
    assert_plane_sum!(evidence_hard_constraint_cases);
    assert_plane_sum!(truth_members);
    assert_plane_sum!(truth_members_in_universe);
    assert_plane_sum!(backbone_true_positive_members);
    assert_plane_sum!(backbone_false_positive_members);
}

#[test]
fn sound_rho_contracts_compile_exact_existential_and_integer_sum_constraints() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
fn building_profile_threads_through_evidence_compilation() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe: building_universe(&["building-c", "building-a", "building-b"]),
        contracts: vec![contract(
            "admitted-building",
            GeoRhoSoundness::LogicallySound,
        )],
        observations: vec![GeoRhoObservation {
            id: "allowed-buildings".to_string(),
            contract_id: "admitted-building".to_string(),
            source_records: vec![source_record("allowed-buildings-row")],
            valid_time: None,
            observation: GeoRhoObservationKind::ExactSets {
                level: GeoEntityLevel::Building,
                sets: vec![
                    vec!["building-b".to_string()],
                    vec!["building-c".to_string(), "building-a".to_string()],
                    vec!["building-a".to_string()],
                ],
            },
        }],
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let compiled = compile_evidence(&request).expect("building evidence must compile");
    assert_eq!(
        compiled.composition_request.profile,
        GeoCompositionProfile::building()
    );
    assert!(compiled.composition_request.universe.parcels.is_empty());
    let solved = solve_composition(&compiled.composition_request).expect("compiled request solves");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(solved.summary.residual_model_count, 3);
    assert_eq!(
        solved
            .residual_models
            .iter()
            .map(|model| model.buildings.clone())
            .collect::<Vec<_>>(),
        [
            vec!["building-a".to_string()],
            vec!["building-a".to_string(), "building-c".to_string()],
            vec!["building-b".to_string()]
        ]
    );
}

#[test]
fn building_profile_with_parcel_universe_refuses_during_evidence_compilation() {
    let error = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe: GeoCompositionUniverse {
            parcels: parcels(&["parcel-a", "parcel-b"]),
            buildings: vec![GeoBuildingCandidate {
                id: "building-a".to_string(),
                parcel_ids: Vec::new(),
            }],
        },
        contracts: Vec::new(),
        observations: Vec::new(),
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect_err("evidence compilation must not create false building-grain counts");

    assert_eq!(error.code, canon::geo::GeoEvidenceErrorCode::Composition);
    assert_eq!(error.detail["composition_code"], "UnsupportedGrain");
    assert_eq!(error.detail["selection_level"], "building");
    assert_eq!(error.detail["field"], "universe.parcels");
}

#[test]
fn evidence_without_immutable_source_records_is_rejected_before_admission() {
    let error = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
        profile: Default::default(),
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
        profile: Default::default(),
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
fn empirical_integer_sum_band_requires_admissible_hard_band_contract() {
    let request = |contract: GeoRhoContract| GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![contract],
        observations: vec![integer_sum_observation(
            "observer-count",
            "rho.structure_count.v0",
        )],
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let unflagged = compile_evidence(&request(contract(
        "rho.structure_count.v0",
        GeoRhoSoundness::EmpiricalHighCoverage,
    )))
    .expect("unflagged empirical band compiles as diagnostic");
    assert!(unflagged.composition_request.hard_constraints.is_empty());
    assert_eq!(
        unflagged.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    assert_eq!(
        unflagged.admissions[0].admission_reason.as_deref(),
        Some("rho_band_not_admissible")
    );
    let unflagged_solve =
        solve_composition(&unflagged.composition_request).expect("unflagged request solves");
    assert_eq!(unflagged_solve.summary.residual_model_count, 7);

    let mut malformed = flagged_empirical_contract("rho.structure_count.v0");
    if let GeoRhoBasis::EmpiricalCalibration {
        calibration_blake3, ..
    } = &mut malformed.basis
    {
        calibration_blake3.clear();
    }
    let error = compile_evidence(&request(malformed))
        .expect_err("hard-band calibration flag requires a calibration digest");
    assert_eq!(error.code, canon::geo::GeoEvidenceErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("contracts[].basis.calibration_blake3")
    );

    let flagged = compile_evidence(&request(flagged_empirical_contract(
        "rho.structure_count.v0",
    )))
    .expect("complete flagged empirical band compiles as hard evidence");
    assert_eq!(flagged.composition_request.hard_constraints.len(), 1);
    assert_eq!(
        flagged.admissions[0].disposition,
        GeoEvidenceDisposition::HardConstraint
    );
    assert_eq!(flagged.admissions[0].admission_reason, None);
    assert_eq!(
        flagged.admissions[0].generated_ids,
        vec!["rho:rho.structure_count.v0@v1:observer-count".to_string()]
    );
    let flagged_solve =
        solve_composition(&flagged.composition_request).expect("flagged request solves");
    assert_eq!(flagged_solve.summary.residual_model_count, 3);
    assert!(
        flagged_solve.summary.residual_model_count < unflagged_solve.summary.residual_model_count
    );
}

#[test]
fn contradictory_sound_observations_name_a_deterministic_conflict() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
fn population_counts_conflict_as_solver_artifact_abstention_not_resolved() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "population-conflict".to_string(),
            evidence,
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: parcels(&["p1"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("conflict is a typed population outcome");

    let case = &artifact.cases[0];
    assert_eq!(case.status, GeoPopulationCaseStatus::Conflict);
    assert!(case.solver_digest.is_some());
    assert!(case.abstained);
    assert_eq!(case.residual_model_count, Some(0));
    assert!(case.residual_count_complete);
    assert!(!case.false_merge);
    assert_eq!(artifact.summary.conflict_cases, 1);
    assert_eq!(artifact.summary.solver_artifact_cases, 1);
    assert_eq!(artifact.summary.abstention_cases, 1);
    assert_eq!(artifact.summary.resolved_cases, 0);
    assert_eq!(artifact.summary.ambiguous_cases, 0);
    assert_eq!(artifact.summary.component_budget_fallback_cases, 0);
    assert_eq!(artifact.summary.assignment_budget_exceeded_cases, 0);
}

#[test]
fn evidence_compilation_is_byte_identical_under_equivalent_permutations() {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
        profile: Default::default(),
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
                truth_plane: GeoTruthPlane::GateV2Historical,
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
        profile: Default::default(),
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
            truth_plane: GeoTruthPlane::GateV2Historical,
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
    assert_eq!(case.candidate_reach, GeoCandidateReachStatus::None);
    assert_eq!(case.truth_model_in_residual, None);
    assert!(!case.solver_truth_scored);
    assert!(case.backbone_complete);
    assert_eq!(
        case.evidence_coverage,
        GeoEvidenceCoverageStatus::NoObservations
    );
    assert!(!case.false_merge);
    assert_eq!(artifact.summary.population_eligible_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_evaluated_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_none_cases, 1);
    assert_eq!(artifact.summary.candidate_recall_failure_cases, 1);
    assert_eq!(artifact.summary.solver_artifact_cases, 1);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 0);
    assert_eq!(artifact.summary.empirical_falsification_eligible_cases, 0);
    assert_eq!(artifact.summary.false_merge_cases, 0);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 0);
}

#[test]
fn population_keeps_partial_candidate_reach_out_of_solver_truth_metrics() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["p1", "p2"]),
        contracts: Vec::new(),
        observations: Vec::new(),
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "partial-candidate-reach".to_string(),
            evidence,
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: parcels(&["p1", "p9"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("partial reach is an upstream evaluation outcome");

    let case = &artifact.cases[0];
    assert_eq!(case.truth_members, 2);
    assert_eq!(case.truth_members_in_universe, 1);
    assert_eq!(case.candidate_reach, GeoCandidateReachStatus::Partial);
    assert!(!case.full_truth_recall);
    assert!(!case.solver_truth_scored);
    assert_eq!(case.truth_model_in_residual, None);
    assert_eq!(case.backbone_true_positive_members, 0);
    assert_eq!(case.backbone_false_positive_members, 0);
    assert!(!case.false_merge);
    assert_eq!(artifact.summary.population_eligible_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_evaluated_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_partial_cases, 1);
    assert_eq!(artifact.summary.candidate_recall_failure_cases, 1);
    assert_eq!(artifact.summary.solver_artifact_cases, 1);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 0);
    assert_eq!(artifact.summary.empirical_falsification_eligible_cases, 0);
    assert_eq!(artifact.summary.false_merge_cases, 0);
}

#[test]
fn population_does_not_score_false_merge_when_truth_is_unreachable() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["p1"]),
        contracts: vec![contract("declared-anchor", GeoRhoSoundness::LogicallySound)],
        observations: vec![GeoRhoObservation {
            id: "forces-p1".to_string(),
            contract_id: "declared-anchor".to_string(),
            source_records: vec![source_record("declared-anchor-row")],
            valid_time: None,
            observation: GeoRhoObservationKind::ExactSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![parcels(&["p1"])],
            },
        }],
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "resolved-but-truth-outside-candidates".to_string(),
            evidence,
            truth_plane: GeoTruthPlane::NonRoundAmountDateLegalBorough,
            truth: GeoCompositionModel {
                parcels: parcels(&["p9"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("candidate reach failure is an upstream evaluation outcome");

    let case = &artifact.cases[0];
    assert_eq!(case.status, GeoPopulationCaseStatus::Resolved);
    assert_eq!(case.candidate_reach, GeoCandidateReachStatus::None);
    assert!(!case.solver_truth_scored);
    assert_eq!(case.truth_model_in_residual, None);
    assert_eq!(
        case.hard_forced.parcels,
        parcels(&["p1"]),
        "the solver result remains reported even though correctness is unscored"
    );
    assert_eq!(case.backbone_true_positive_members, 0);
    assert_eq!(case.backbone_false_positive_members, 0);
    assert!(!case.false_merge);
    assert_eq!(artifact.summary.resolved_cases, 1);
    assert_eq!(artifact.summary.population_eligible_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_evaluated_cases, 1);
    assert_eq!(artifact.summary.candidate_recall_failure_cases, 1);
    assert_eq!(artifact.summary.solver_artifact_cases, 1);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 0);
    assert_eq!(artifact.summary.empirical_falsification_eligible_cases, 0);
    assert_eq!(artifact.summary.false_merge_cases, 0);
}

#[test]
fn population_reports_diagnostic_and_soft_diagnostic_evidence_coverage_separately() {
    let diagnostic_only = GeoLabeledCompositionCase {
        id: "coverage-diagnostic-only".to_string(),
        evidence: GeoEvidenceCompilationRequest {
            version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
            profile: Default::default(),
            universe: universe(&["p1", "p2"]),
            contracts: vec![contract(
                "empirical-diagnostic",
                GeoRhoSoundness::EmpiricalHighCoverage,
            )],
            observations: vec![GeoRhoObservation {
                id: "would-prune-if-misclassified".to_string(),
                contract_id: "empirical-diagnostic".to_string(),
                source_records: vec![
                    source_record("diagnostic-row-a"),
                    source_record("diagnostic-row-b"),
                ],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![parcels(&["p2"])],
                },
            }],
            max_assignments: 4,
            max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
        },
        truth_plane: GeoTruthPlane::GateV2Historical,
        truth: GeoCompositionModel {
            parcels: parcels(&["p1"]),
            buildings: Vec::new(),
        },
    };
    let soft_and_diagnostic = GeoLabeledCompositionCase {
        id: "coverage-soft-and-diagnostic".to_string(),
        evidence: GeoEvidenceCompilationRequest {
            version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
            profile: Default::default(),
            universe: universe(&["p1", "p2"]),
            contracts: vec![contract(
                "empirical-mixed",
                GeoRhoSoundness::EmpiricalHighCoverage,
            )],
            observations: vec![
                GeoRhoObservation {
                    id: "rank-p1".to_string(),
                    contract_id: "empirical-mixed".to_string(),
                    source_records: vec![source_record("soft-row")],
                    valid_time: None,
                    observation: GeoRhoObservationKind::PreferMember {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p1"),
                        cost_if_absent: 1,
                    },
                },
                GeoRhoObservation {
                    id: "diagnostic-p2".to_string(),
                    contract_id: "empirical-mixed".to_string(),
                    source_records: vec![source_record("mixed-diagnostic-row")],
                    valid_time: None,
                    observation: GeoRhoObservationKind::ExactSets {
                        level: GeoEntityLevel::Parcel,
                        sets: vec![parcels(&["p2"])],
                    },
                },
            ],
            max_assignments: 4,
            max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
        },
        truth_plane: GeoTruthPlane::GateV2Historical,
        truth: GeoCompositionModel {
            parcels: parcels(&["p1"]),
            buildings: Vec::new(),
        },
    };

    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![diagnostic_only, soft_and_diagnostic],
        max_cases: 2,
    })
    .expect("coverage bucket evaluation must succeed");
    let diagnostic_case = artifact
        .cases
        .iter()
        .find(|case| case.case_id == "coverage-diagnostic-only")
        .expect("diagnostic-only case");
    let mixed_case = artifact
        .cases
        .iter()
        .find(|case| case.case_id == "coverage-soft-and-diagnostic")
        .expect("soft+diagnostic case");

    assert_eq!(
        diagnostic_case.evidence_coverage,
        GeoEvidenceCoverageStatus::DiagnosticOnly
    );
    assert_eq!(diagnostic_case.evidence_observations, 1);
    assert_eq!(
        diagnostic_case.evidence_records, 2,
        "source-record volume is provenance volume, not confidence"
    );
    assert_eq!(diagnostic_case.hard_constraint_observations, 0);
    assert_eq!(diagnostic_case.soft_preference_observations, 0);
    assert_eq!(diagnostic_case.diagnostic_observations, 1);

    assert_eq!(
        mixed_case.evidence_coverage,
        GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly
    );
    assert_eq!(mixed_case.evidence_observations, 2);
    assert_eq!(mixed_case.evidence_records, 2);
    assert_eq!(mixed_case.hard_constraint_observations, 0);
    assert_eq!(mixed_case.soft_preference_observations, 1);
    assert_eq!(mixed_case.diagnostic_observations, 1);

    assert_eq!(artifact.summary.evidence_diagnostic_only_cases, 1);
    assert_eq!(artifact.summary.evidence_soft_and_diagnostic_only_cases, 1);
    assert_eq!(artifact.summary.evidence_hard_constraint_cases, 0);
    let plane = artifact
        .summary
        .truth_planes
        .iter()
        .find(|summary| summary.truth_plane == GeoTruthPlane::GateV2Historical)
        .expect("gate v2 summary");
    assert_eq!(plane.evidence_diagnostic_only_cases, 1);
    assert_eq!(plane.evidence_soft_and_diagnostic_only_cases, 1);
    assert_eq!(plane.evidence_hard_constraint_cases, 0);
}

#[test]
fn population_summarizes_h7_truth_planes_without_pooling() {
    let hard_case =
        |id: &str, plane: GeoTruthPlane, forced: &str, truth: &str| GeoLabeledCompositionCase {
            id: id.to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: Default::default(),
                universe: universe(&["p1", "p2", "p3"]),
                contracts: vec![contract(id, GeoRhoSoundness::LogicallySound)],
                observations: vec![GeoRhoObservation {
                    id: "declared-set".to_string(),
                    contract_id: id.to_string(),
                    source_records: vec![source_record(&format!("{id}-row"))],
                    valid_time: None,
                    observation: GeoRhoObservationKind::ExistentialMembership {
                        members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, forced)],
                    },
                }],
                max_assignments: 8,
                max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
            },
            truth_plane: plane,
            truth: GeoCompositionModel {
                parcels: parcels(&[truth]),
                buildings: Vec::new(),
            },
        };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![
            hard_case(
                "h7-non-round",
                GeoTruthPlane::NonRoundAmountDateLegalBorough,
                "p1",
                "p1",
            ),
            hard_case(
                "h7-round-exact-lender",
                GeoTruthPlane::RoundExactLenderParty,
                "p1",
                "p3",
            ),
        ],
        max_cases: 2,
    })
    .expect("truth-plane split must evaluate");

    assert_eq!(artifact.summary.cases, 2);
    assert_eq!(artifact.summary.truth_planes.len(), 2);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 2);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 1);
    assert_eq!(artifact.summary.evidence_hard_constraint_cases, 2);

    let non_round = artifact
        .summary
        .truth_planes
        .iter()
        .find(|summary| summary.truth_plane == GeoTruthPlane::NonRoundAmountDateLegalBorough)
        .expect("non-round truth plane summary");
    let round = artifact
        .summary
        .truth_planes
        .iter()
        .find(|summary| summary.truth_plane == GeoTruthPlane::RoundExactLenderParty)
        .expect("round exact-lender truth plane summary");
    assert_eq!(non_round.cases, 1);
    assert_eq!(non_round.solver_truth_exclusion_cases, 0);
    assert_eq!(non_round.candidate_reach_full_cases, 1);
    assert_eq!(round.cases, 1);
    assert_eq!(round.solver_truth_exclusion_cases, 1);
    assert_eq!(round.candidate_reach_full_cases, 1);
}

#[test]
fn population_denominators_keep_reach_feasibility_and_falsification_disjoint_by_truth_plane() {
    let artifact = mixed_denominator_population();

    assert_eq!(artifact.summary.cases, 3);
    assert_eq!(artifact.summary.population_eligible_cases, 3);
    assert_eq!(artifact.summary.candidate_reach_evaluated_cases, 3);
    assert_eq!(artifact.summary.candidate_reach_full_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_partial_cases, 1);
    assert_eq!(artifact.summary.candidate_reach_none_cases, 1);
    assert_eq!(artifact.summary.candidate_recall_failure_cases, 2);
    assert_eq!(artifact.summary.solver_artifact_cases, 3);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 1);
    assert_eq!(artifact.summary.empirical_falsification_eligible_cases, 1);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 1);
    assert_summary_matches_truth_plane_sums(&artifact.summary);

    let gate = artifact
        .summary
        .truth_planes
        .iter()
        .find(|summary| summary.truth_plane == GeoTruthPlane::GateV2Historical)
        .expect("Gate V2 plane summary");
    assert_eq!(gate.cases, 2);
    assert_eq!(gate.population_eligible_cases, 2);
    assert_eq!(gate.candidate_reach_evaluated_cases, 2);
    assert_eq!(gate.candidate_reach_full_cases, 0);
    assert_eq!(gate.candidate_reach_partial_cases, 1);
    assert_eq!(gate.candidate_reach_none_cases, 1);
    assert_eq!(gate.solver_artifact_cases, 2);
    assert_eq!(gate.solver_truth_scored_cases, 0);
    assert_eq!(gate.empirical_falsification_eligible_cases, 0);
    assert_eq!(gate.solver_truth_exclusion_cases, 0);

    let round = artifact
        .summary
        .truth_planes
        .iter()
        .find(|summary| summary.truth_plane == GeoTruthPlane::RoundExactLenderParty)
        .expect("round truth plane summary");
    assert_eq!(round.cases, 1);
    assert_eq!(round.population_eligible_cases, 1);
    assert_eq!(round.candidate_reach_evaluated_cases, 1);
    assert_eq!(round.candidate_reach_full_cases, 1);
    assert_eq!(round.solver_artifact_cases, 1);
    assert_eq!(round.solver_truth_scored_cases, 1);
    assert_eq!(round.empirical_falsification_eligible_cases, 1);
    assert_eq!(round.solver_truth_exclusion_cases, 1);
}

#[test]
fn population_evaluation_validator_rejects_truth_plane_shared_counter_tamper() {
    let mut artifact = mixed_denominator_population();
    validate_population_evaluation_artifact(&artifact).expect("fixture artifact is valid");

    artifact.summary.truth_planes[0].solver_artifact_cases += 1;
    let error = validate_population_evaluation_artifact(&artifact)
        .expect_err("truth-plane shared-counter tamper must be refused");

    assert_eq!(error.code, GeoPopulationErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("scope").map(String::as_str),
        Some("truth_planes_sum")
    );
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("solver_artifact_cases")
    );
}

#[test]
fn population_counts_ambiguous_truth_exclusions_as_rho_contract_falsifications() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
            truth_plane: GeoTruthPlane::GateV2Historical,
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
fn population_reports_ambiguous_backbone_false_positive_without_false_merge() {
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["p1", "p2", "p3"]),
        contracts: vec![contract("allowed-sets", GeoRhoSoundness::LogicallySound)],
        observations: vec![GeoRhoObservation {
            id: "p1-plus-one".to_string(),
            contract_id: "allowed-sets".to_string(),
            source_records: vec![source_record("allowed-sets-row")],
            valid_time: None,
            observation: GeoRhoObservationKind::ExactSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![parcels(&["p1", "p2"]), parcels(&["p1", "p3"])],
            },
        }],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let artifact = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "ambiguous-backbone-fp".to_string(),
            evidence,
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: parcels(&["p2", "p3"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    })
    .expect("ambiguous backbone false positives remain reportable diagnostics");

    let case = &artifact.cases[0];
    assert_eq!(case.status, GeoPopulationCaseStatus::Ambiguous);
    assert_eq!(case.truth_model_in_residual, Some(false));
    assert_eq!(case.hard_forced.parcels, parcels(&["p1"]));
    assert!(case.backbone_complete);
    assert_eq!(case.backbone_true_positive_members, 0);
    assert_eq!(case.backbone_false_positive_members, 1);
    assert!(!case.false_merge);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 1);
    assert_eq!(artifact.summary.backbone_false_positive_members, 1);
    assert_eq!(artifact.summary.false_merge_cases, 0);
}

#[test]
fn population_never_reports_budget_fallback_placeholder_zero_as_a_model_count() {
    let parcel_ids = (0..12)
        .map(|index| format!("p{index:02}"))
        .collect::<Vec<_>>();
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
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
            truth_plane: GeoTruthPlane::GateV2Historical,
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
    assert!(!case.residual_count_complete);
    assert!(!case.residual_count_saturated);
    assert!(case.solver_digest.is_some());
    assert_eq!(case.truth_model_in_residual, Some(true));
    assert!(case.solver_truth_scored);
    assert!(!case.backbone_complete);
    assert!(case.abstained);
    assert_eq!(artifact.summary.component_budget_fallback_cases, 1);
    assert_eq!(artifact.summary.solver_artifact_cases, 1);
    assert_eq!(artifact.summary.abstention_cases, 1);
    assert_eq!(artifact.summary.resolved_cases, 0);
    assert_eq!(artifact.summary.ambiguous_cases, 0);
    assert_eq!(artifact.summary.conflict_cases, 0);
    assert_eq!(artifact.summary.assignment_budget_exceeded_cases, 0);
    assert_eq!(artifact.summary.solver_truth_scored_cases, 1);
    assert_eq!(artifact.summary.solver_truth_exclusion_cases, 0);
    assert_eq!(artifact.summary.residual_count_unavailable_cases, 1);
    assert_eq!(artifact.summary.backbone_complete_cases, 0);
}

#[test]
fn population_rejects_empty_truth_labels_instead_of_calling_them_full_recall() {
    let error = evaluate_population(&GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "empty-truth".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: Default::default(),
                universe: universe(&["p1"]),
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 2,
                max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
            },
            truth_plane: GeoTruthPlane::GateV2Historical,
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
                    profile: Default::default(),
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
                truth_plane: GeoTruthPlane::GateV2Historical,
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
