use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate,
    GeoCompositionErrorCode, GeoCompositionModel, GeoCompositionRequest, GeoCompositionStatus,
    GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint, GeoHardConstraintKind,
    GeoIdentityRelation, GeoIntegerMemberValue, canonical_composition_bytes,
    model_satisfies_request, solve_composition, validate_identity_relation,
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
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let artifact = solve_composition(&request).expect("conflict is a domain result");
    assert_eq!(artifact.status, GeoCompositionStatus::Conflict);
    assert!(artifact.residual_models.is_empty());
    assert_eq!(artifact.conflict_constraint_ids, ["a_forbid", "z_only_a"]);
}

#[test]
fn global_mask_overflow_now_solves_exactly() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: (0..10).map(|index| format!("parcel-{index:02}")).collect(),
            buildings: Vec::new(),
        },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 100,
        max_materialized_models: 0,
    };

    // The v1 solver decomposes free variables into singleton components, so
    // a universe whose global mask space exceeds the budget no longer
    // refuses: it solves exactly and reports the residual compactly.
    let artifact = solve_composition(&request).expect("free variables solve exactly");
    assert_eq!(artifact.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(artifact.summary.residual_model_count, 1_023);
    assert_eq!(artifact.summary.component_count, 10);
    assert!(!artifact.summary.residual_models_materialized);
    assert!(artifact.budget_fallback.is_none());
    assert!(artifact.hard_forced.parcels.is_empty());
}

#[test]
fn zero_assignment_budget_refuses_validation() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: vec!["parcel-a".to_string()],
            buildings: Vec::new(),
        },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 0,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let error = solve_composition(&request).expect_err("zero budget must refuse");
    assert_eq!(error.code, GeoCompositionErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "max_assignments");
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

// ---------------------------------------------------------------------------
// bd-2kjx.3 verification: factorized solver vs brute-force oracle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 16
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn member_in(model: &GeoCompositionModel, member: &GeoEntityRef) -> bool {
    match member.level {
        GeoEntityLevel::Parcel => model.parcels.binary_search(&member.id).is_ok(),
        GeoEntityLevel::Building => model.buildings.binary_search(&member.id).is_ok(),
        _ => false,
    }
}

fn oracle_holds(model: &GeoCompositionModel, kind: &GeoHardConstraintKind) -> bool {
    match kind {
        GeoHardConstraintKind::Require { member } => member_in(model, member),
        GeoHardConstraintKind::Forbid { member } => !member_in(model, member),
        GeoHardConstraintKind::Cardinality { level, min, max } => {
            let members = match level {
                GeoEntityLevel::Parcel => &model.parcels,
                _ => &model.buildings,
            };
            (*min..=*max).contains(&members.len())
        }
        GeoHardConstraintKind::AllowedSets { level, sets } => {
            let members = match level {
                GeoEntityLevel::Parcel => &model.parcels,
                _ => &model.buildings,
            };
            sets.iter().any(|allowed| allowed == members)
        }
        GeoHardConstraintKind::AnyOf { members } => {
            members.iter().any(|member| member_in(model, member))
        }
        GeoHardConstraintKind::IntegerSumBand {
            level,
            values,
            min,
            max,
        } => {
            let members = match level {
                GeoEntityLevel::Parcel => &model.parcels,
                _ => &model.buildings,
            };
            let sum: u64 = values
                .iter()
                .filter(|value| members.binary_search(&value.id).is_ok())
                .map(|value| value.value)
                .sum();
            (*min..=*max).contains(&sum)
        }
        GeoHardConstraintKind::AllOrNone { members } => {
            let selected = members
                .iter()
                .filter(|member| member_in(model, member))
                .count();
            selected == 0 || selected == members.len()
        }
        GeoHardConstraintKind::Requires {
            if_member,
            then_member,
        } => !member_in(model, if_member) || member_in(model, then_member),
    }
}

/// Reference implementation of the v0 kernel semantics: enumerate every
/// inclusion mask, keep at-least-one-parcel plus containment-feasible
/// selections, filter by every constraint, sort.
fn oracle_residual(
    parcels: &[String],
    buildings: &[GeoBuildingCandidate],
    constraints: &[GeoHardConstraint],
) -> Vec<GeoCompositionModel> {
    let parcel_count = parcels.len();
    let total = parcel_count + buildings.len();
    let mut models = Vec::new();
    for mask in 0..(1_u128 << total) {
        let selected_parcels: Vec<String> = parcels
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u128 << index) != 0)
            .map(|(_, id)| id.clone())
            .collect();
        if selected_parcels.is_empty() {
            continue;
        }
        let selected_buildings: Vec<&GeoBuildingCandidate> = buildings
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u128 << (parcel_count + index)) != 0)
            .map(|(_, building)| building)
            .collect();
        if selected_buildings.iter().any(|building| {
            !building.parcel_ids.is_empty()
                && !building
                    .parcel_ids
                    .iter()
                    .any(|parcel_id| selected_parcels.contains(parcel_id))
        }) {
            continue;
        }
        let model = GeoCompositionModel {
            parcels: selected_parcels,
            buildings: selected_buildings
                .iter()
                .map(|building| building.id.clone())
                .collect(),
        };
        if constraints
            .iter()
            .all(|constraint| oracle_holds(&model, &constraint.constraint))
        {
            models.push(model);
        }
    }
    models.sort_unstable();
    models
}

fn random_constraints(
    rng: &mut Lcg,
    parcels: &[String],
    buildings: &[GeoBuildingCandidate],
) -> Vec<GeoHardConstraint> {
    let parcel_ref = |rng: &mut Lcg| {
        GeoEntityRef::new(
            GeoEntityLevel::Parcel,
            parcels[rng.below(parcels.len())].clone(),
        )
    };
    let building_ref = |rng: &mut Lcg| {
        GeoEntityRef::new(
            GeoEntityLevel::Building,
            buildings[rng.below(buildings.len())].id.clone(),
        )
    };
    let any_ref = |rng: &mut Lcg| {
        if buildings.is_empty() || rng.below(2) == 0 {
            parcel_ref(rng)
        } else {
            building_ref(rng)
        }
    };
    let count = rng.below(5);
    let mut constraints = Vec::new();
    for index in 0..count {
        let kind = match rng.below(7) {
            0 => GeoHardConstraintKind::Require {
                member: any_ref(rng),
            },
            1 => GeoHardConstraintKind::Forbid {
                member: any_ref(rng),
            },
            2 => {
                let level = if rng.below(2) == 0 || buildings.is_empty() {
                    GeoEntityLevel::Parcel
                } else {
                    GeoEntityLevel::Building
                };
                let available = match level {
                    GeoEntityLevel::Parcel => parcels.len(),
                    _ => buildings.len(),
                };
                let min = rng.below(available + 1);
                let max = min + rng.below(available - min + 1);
                GeoHardConstraintKind::Cardinality { level, min, max }
            }
            3 => {
                let level = if rng.below(2) == 0 || buildings.is_empty() {
                    GeoEntityLevel::Parcel
                } else {
                    GeoEntityLevel::Building
                };
                let pool: Vec<String> = match level {
                    GeoEntityLevel::Parcel => parcels.to_vec(),
                    _ => buildings.iter().map(|b| b.id.clone()).collect(),
                };
                let set_count = 1 + rng.below(pool.len() + 1);
                let mut sets: Vec<Vec<String>> = (0..set_count)
                    .map(|_| {
                        pool.iter()
                            .filter(|_| rng.below(2) == 0)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .collect();
                sets.sort();
                sets.dedup();
                if sets.is_empty() {
                    continue;
                }
                GeoHardConstraintKind::AllowedSets { level, sets }
            }
            4 => {
                let mut members = (0..1 + rng.below(3.min(parcels.len() + buildings.len())))
                    .map(|_| any_ref(rng))
                    .collect::<Vec<_>>();
                members.sort_by(|left, right| {
                    format!("{}:{}", left.level as u8, left.id)
                        .cmp(&format!("{}:{}", right.level as u8, right.id))
                });
                members.dedup();
                if members.is_empty() {
                    continue;
                }
                GeoHardConstraintKind::AnyOf { members }
            }
            5 => {
                let picked: Vec<(String, u64)> = parcels
                    .iter()
                    .filter_map(|id| {
                        if rng.below(2) == 0 {
                            Some((id.clone(), 1 + rng.next_u64() % 500))
                        } else {
                            None
                        }
                    })
                    .collect();
                if picked.is_empty() {
                    continue;
                }
                let total: u64 = picked.iter().map(|(_, value)| value).sum();
                GeoHardConstraintKind::IntegerSumBand {
                    level: GeoEntityLevel::Parcel,
                    values: picked
                        .into_iter()
                        .map(|(id, value)| GeoIntegerMemberValue { id, value })
                        .collect(),
                    min: 0,
                    max: total,
                }
            }
            _ => {
                if buildings.is_empty() && parcels.len() < 2 {
                    continue;
                }
                let first = any_ref(rng);
                let second = any_ref(rng);
                if first == second {
                    continue;
                }
                GeoHardConstraintKind::AllOrNone {
                    members: vec![first, second],
                }
            }
        };
        constraints.push(GeoHardConstraint {
            id: format!("c{index:02}"),
            constraint: kind,
        });
    }
    constraints.sort_by(|left, right| left.id.cmp(&right.id));
    constraints.dedup_by(|left, right| left.id == right.id);
    constraints.retain(|constraint| !constraint.id.is_empty());
    constraints
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
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let artifact = solve_composition(&request).expect("structural conflict must report");
    assert_eq!(artifact.status, GeoCompositionStatus::Conflict);
    assert!(artifact.residual_models.is_empty());
}

#[test]
fn factorized_solver_matches_brute_force_oracle_on_random_universes() {
    let mut rng = Lcg(0x5EED_1A12);
    for iteration in 0..300 {
        let parcel_count = 1 + rng.below(5);
        let building_count = rng.below(5);
        let parcels: Vec<String> = (0..parcel_count).map(|index| format!("p{index}")).collect();
        let buildings: Vec<GeoBuildingCandidate> = (0..building_count)
            .map(|index| {
                let link_count = rng.below(parcel_count + 1);
                let mut ids: Vec<String> = (0..link_count)
                    .map(|offset| {
                        format!(
                            "p{}",
                            (index * 7 + offset * 3 + rng.below(parcel_count)) % parcel_count
                        )
                    })
                    .collect();
                ids.sort();
                ids.dedup();
                GeoBuildingCandidate {
                    id: format!("b{index}"),
                    parcel_ids: ids,
                }
            })
            .collect();
        let constraints = random_constraints(&mut rng, &parcels, &buildings);

        let request = GeoCompositionRequest {
            version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
            universe: GeoCompositionUniverse {
                parcels: parcels.clone(),
                buildings: buildings.clone(),
            },
            hard_constraints: constraints.clone(),
            soft_preferences: Vec::new(),
            max_assignments: 1 << 20,
            max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
        };
        let artifact = solve_composition(&request)
            .unwrap_or_else(|error| panic!("iteration {iteration} must solve: {error}"));

        let expected = oracle_residual(&parcels, &buildings, &constraints);
        let expected_count = expected.len() as u64;
        let expected_status = match expected.len() {
            0 => GeoCompositionStatus::Conflict,
            1 => GeoCompositionStatus::Resolved,
            _ => GeoCompositionStatus::Ambiguous,
        };
        assert_eq!(artifact.status, expected_status, "iteration {iteration}");
        assert_eq!(
            artifact.summary.residual_model_count, expected_count,
            "iteration {iteration}"
        );
        assert_eq!(artifact.residual_models, expected, "iteration {iteration}");
        if expected_count > 0 {
            let first = &expected[0];
            let backbone_parcels: Vec<String> = first
                .parcels
                .iter()
                .filter(|id| expected.iter().all(|model| model.parcels.contains(id)))
                .cloned()
                .collect();
            let backbone_buildings: Vec<String> = first
                .buildings
                .iter()
                .filter(|id| expected.iter().all(|model| model.buildings.contains(id)))
                .cloned()
                .collect();
            assert_eq!(artifact.hard_forced.parcels, backbone_parcels);
            assert_eq!(artifact.hard_forced.buildings, backbone_buildings);
        }
    }
}

fn star_universe(star_count: usize) -> GeoCompositionRequest {
    let parcels: Vec<String> = (0..star_count)
        .map(|index| format!("p{index:02}"))
        .collect();
    let buildings: Vec<GeoBuildingCandidate> = (0..star_count)
        .map(|index| GeoBuildingCandidate {
            id: format!("b{index:02}"),
            parcel_ids: vec![format!("p{index:02}")],
        })
        .collect();
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse { parcels, buildings },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 64,
        max_materialized_models: 0,
    }
}

#[test]
fn measured_block_shapes_solve_without_cartesian_refusal() {
    // 30 independent parcel-star pairs (60 variables). The v0 kernel refused
    // at 2^60 masks; decomposition gives exact count 3^30 - 1.
    let request = star_universe(30);
    let artifact = solve_composition(&request).expect("stars must decompose");
    assert_eq!(artifact.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(artifact.summary.component_count, 30);
    let expected_count = 3_u128.pow(30) - 1;
    assert_eq!(
        u128::from(artifact.summary.residual_model_count),
        expected_count
    );
    assert!(!artifact.summary.summary_counts_saturated);
    assert!(!artifact.summary.residual_models_materialized);
    assert!(artifact.soft_ranked.is_empty());
    assert!(artifact.hard_forced.parcels.is_empty());
}

#[test]
fn extreme_universe_reports_saturated_lower_bound_instead_of_guessing() {
    // 46 stars = 92 variables, the measured maximum block shape. The exact
    // residual 3^46 - 1 exceeds u64 range; report the declared bound.
    let request = star_universe(46);
    let artifact = solve_composition(&request).expect("92 variables must still solve");
    assert_eq!(artifact.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(artifact.summary.component_count, 46);
    assert_eq!(artifact.summary.residual_model_count, u64::MAX);
    assert!(artifact.summary.summary_counts_saturated);
}

#[test]
fn oversized_component_budget_fallback_is_typed_and_deterministic() {
    // One global AllowedSets couples all 12 parcels into a single component
    // whose 2^12 space exceeds the budget; AllowedSets prunes nothing, so
    // the bounded search must exhaust into the typed handoff.
    let parcels: Vec<String> = (0..12).map(|index| format!("p{index:02}")).collect();
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: parcels.clone(),
            buildings: Vec::new(),
        },
        hard_constraints: vec![GeoHardConstraint {
            id: "whole_level".to_string(),
            constraint: GeoHardConstraintKind::AllowedSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![parcels.clone()],
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 1_000,
        max_materialized_models: 0,
    };
    let artifact = solve_composition(&request).expect("fallback is a domain outcome");
    assert_eq!(artifact.status, GeoCompositionStatus::BudgetFallback);
    let fallback = artifact.budget_fallback.as_ref().expect("typed handoff");
    assert_eq!(fallback.max_component_variables, 12);
    assert_eq!(fallback.component_keys.len(), 1);
    assert!(fallback.guidance.contains("max_assignments"));
    assert!(artifact.residual_models.is_empty());
    assert!(artifact.hard_forced.parcels.is_empty());

    let repeat = solve_composition(&request).expect("repeat must agree");
    assert_eq!(
        canonical_composition_bytes(&artifact).expect("serialize"),
        canonical_composition_bytes(&repeat).expect("serialize")
    );
}

#[test]
fn bounded_search_completes_when_pruning_collapses_the_tree() {
    // Same single-component shape as the fallback test, but Forbid pins
    // eleven parcels and Require forces p00; partial-feasibility pruning
    // finishes far under budget with an exact singleton residual.
    let parcels: Vec<String> = (0..12).map(|index| format!("p{index:02}")).collect();
    let mut constraints: Vec<GeoHardConstraint> = parcels
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, id)| GeoHardConstraint {
            id: format!("forbid-{index:02}"),
            constraint: GeoHardConstraintKind::Forbid {
                member: GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()),
            },
        })
        .collect();
    constraints.push(GeoHardConstraint {
        id: "require-p00".to_string(),
        constraint: GeoHardConstraintKind::Require {
            member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p00".to_string()),
        },
    });
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels,
            buildings: Vec::new(),
        },
        hard_constraints: constraints,
        soft_preferences: Vec::new(),
        max_assignments: 1_000,
        max_materialized_models: 0,
    };
    let artifact = solve_composition(&request).expect("pruned search must finish");
    assert_eq!(artifact.status, GeoCompositionStatus::Resolved);
    assert_eq!(artifact.summary.residual_model_count, 1);
    assert!(!artifact.summary.summary_counts_saturated);
    assert!(artifact.budget_fallback.is_none());
    assert_eq!(artifact.hard_forced.parcels, ["p00"]);
}

#[test]
fn membership_predicate_decides_residual_exactness_without_materialization() {
    let base = |max_materialized_models: u64| GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: ["a", "b", "c"].iter().map(|id| id.to_string()).collect(),
            buildings: vec![GeoBuildingCandidate {
                id: "x".to_string(),
                parcel_ids: vec!["a".to_string()],
            }],
        },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 256,
        max_materialized_models,
    };
    let model = |parcels: &[&str], buildings: &[&str]| GeoCompositionModel {
        parcels: parcels.iter().map(|id| id.to_string()).collect(),
        buildings: buildings.iter().map(|id| id.to_string()).collect(),
    };
    for request in [base(DEFAULT_MAX_MATERIALIZED_MODELS), base(0)] {
        assert!(model_satisfies_request(&request, &model(&["a"], &[])).expect("valid request"));
        assert!(model_satisfies_request(&request, &model(&["a"], &["x"])).expect("valid request"));
        assert!(
            model_satisfies_request(&request, &model(&["b"], &[])).expect("valid request"),
            "no containment duty until x is selected"
        );
        assert!(
            !model_satisfies_request(&request, &model(&["b"], &["x"])).expect("valid request"),
            "building x requires parcel a"
        );
        assert!(!model_satisfies_request(&request, &model(&["b"], &["x"])).expect("valid request"));
        assert!(
            !model_satisfies_request(&request, &model(&[], &[])).expect("valid request"),
            "at least one parcel is structural"
        );
        assert!(
            !model_satisfies_request(&request, &model(&["zz"], &[])).expect("valid request"),
            "foreign members are outside the universe"
        );
    }
}
