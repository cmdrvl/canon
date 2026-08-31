use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_RESIDUAL_BENCHMARK_VERSION,
    CANON_GEO_RESIDUAL_OBDD_VERSION, GeoBuildingCandidate, GeoCompositionModel,
    GeoCompositionRequest, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint,
    GeoHardConstraintKind, GeoResidualAnswerSetComparison, GeoResidualBackboneComparison,
    GeoResidualBenchmarkCase, GeoResidualBenchmarkErrorCode, GeoResidualBenchmarkInput,
    GeoResidualCountComparison, GeoResidualObddArtifact, GeoResidualObddNode,
    GeoResidualShapeBasis, GeoResidualVariableOrder, compile_geo_residual_obdd,
    geo_residual_measured_star_case, geo_residual_order_sensitivity_case,
    run_geo_residual_benchmark, verify_geo_residual_obdd,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
}

#[derive(Serialize)]
struct TestObddBuildView<'a> {
    version: &'a str,
    request_blake3: &'a str,
    order_name: &'a str,
    variables: &'a [GeoEntityRef],
    root: u32,
    nodes: &'a [GeoResidualObddNode],
}

fn refresh_obdd_digest(obdd: &mut GeoResidualObddArtifact) {
    let bytes = serde_json::to_vec(&TestObddBuildView {
        version: CANON_GEO_RESIDUAL_OBDD_VERSION,
        request_blake3: obdd.request_blake3.as_str(),
        order_name: obdd.order_name.as_str(),
        variables: obdd.variables.as_slice(),
        root: obdd.root,
        nodes: obdd.nodes.as_slice(),
    })
    .expect("test build view serializes");
    obdd.deterministic_build_bytes = u64::try_from(bytes.len()).expect("test artifact fits u64");
    obdd.build_blake3 = format!("blake3:{}", blake3::hash(&bytes).to_hex());
}

fn worked_case(case_id: &str) -> GeoResidualBenchmarkCase {
    let corpus: WorkedCorpus =
        serde_json::from_str(include_str!("fixtures/geo/e4_worked_cases.json"))
            .expect("worked corpus parses");
    let case = corpus
        .cases
        .into_iter()
        .find(|case| case.case_id == case_id)
        .expect("case exists");
    GeoResidualBenchmarkCase {
        case_id: case.case_id,
        source: "tests/fixtures/geo/e4_worked_cases.json".to_string(),
        shape_basis: GeoResidualShapeBasis::RetainedWorkedCase,
        measurement_basis: "Appendix I worked corpus; retained real query-selected case."
            .to_string(),
        request: case.request,
        truth_models: vec![GeoCompositionModel {
            parcels: vec!["1014477501".to_string()],
            buildings: vec!["1076314".to_string()],
        }],
        orders: Vec::new(),
    }
}

fn small_input() -> GeoResidualBenchmarkInput {
    GeoResidualBenchmarkInput {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: "bd-19wp-test".to_string(),
        cases: vec![
            worked_case("case_6_dense_one_parcel_multi_building"),
            geo_residual_measured_star_case(
                "d11_brooklyn_dense_r9_component_width_5",
                "scripts/geo_measurements/README.md D.11: Brooklyn dense r9 max incidence component 5.",
                5,
                64,
            ),
            geo_residual_order_sensitivity_case(8),
        ],
        orders: vec![
            GeoResidualVariableOrder::Canonical,
            GeoResidualVariableOrder::IncidenceInterleaved,
        ],
        max_answer_set_models: 512,
    }
}

#[test]
fn benchmark_proves_equivalence_before_marking_metrics_comparable() {
    let report = run_geo_residual_benchmark(&small_input()).expect("benchmark runs");
    let repeat = run_geo_residual_benchmark(&small_input()).expect("repeat runs");

    assert_eq!(report.version, CANON_GEO_RESIDUAL_BENCHMARK_VERSION);
    assert_eq!(report.input_blake3, repeat.input_blake3);
    assert_eq!(report.sdd_status.status, "not_run");

    for (case, repeat_case) in report.cases.iter().zip(repeat.cases.iter()) {
        assert_eq!(case.request_blake3, repeat_case.request_blake3);
        for (order, repeat_order) in case.orders.iter().zip(repeat_case.orders.iter()) {
            assert_eq!(order.build_blake3, repeat_order.build_blake3);
            assert_eq!(
                order.deterministic_build_bytes,
                order.final_serialized_build_bytes
            );
            assert_eq!(
                order.final_serialized_nonterminal_node_count,
                order.root_reachable_nonterminal_node_count
            );
            assert_eq!(
                order.final_serialized_node_count - order.root_reachable_node_count,
                order.fixed_terminal_overhead_node_count
            );
            assert!(order.fixed_terminal_overhead_node_count <= 1);
            assert!(order.construction_arena_node_count >= order.final_serialized_node_count);
            assert!(order.construction_peak_node_count >= order.construction_arena_node_count);
            assert!(order.equivalence.request_digest_matches);
            assert_eq!(
                order.equivalence.model_count,
                GeoResidualCountComparison::Matches
            );
            assert!(order.equivalence.truth_membership_matches);
            assert_eq!(
                order.equivalence.backbone,
                GeoResidualBackboneComparison::Matches
            );
            assert!(order.formula_comparable_to_search);
            if order.metrics_comparable_to_search {
                assert!(matches!(
                    order.equivalence.answer_sets,
                    GeoResidualAnswerSetComparison::Matches { .. }
                ));
            }
        }
    }
    assert!(
        report
            .cases
            .iter()
            .flat_map(|case| case.orders.iter())
            .any(|order| order.construction_arena_node_count > order.final_serialized_node_count),
        "fixture should expose unreachable construction-arena nodes"
    );
}

#[test]
fn adversarial_pair_case_exposes_order_sensitivity_without_freezing_an_order() {
    let pair_count = 12;
    let input = GeoResidualBenchmarkInput {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: "bd-19wp-order-sensitivity".to_string(),
        cases: vec![geo_residual_order_sensitivity_case(pair_count)],
        orders: Vec::new(),
        max_answer_set_models: 0,
    };

    let report = run_geo_residual_benchmark(&input).expect("benchmark runs");
    let orders = &report.cases[0].orders;
    let interleaved = orders
        .iter()
        .find(|order| order.order_name == "explicit:explicit_interleaved_pairs")
        .expect("interleaved order");
    let grouped = orders
        .iter()
        .find(|order| order.order_name == "explicit:explicit_grouped_pairs")
        .expect("grouped order");

    for order in [interleaved, grouped] {
        assert!(order.equivalence.request_digest_matches);
        assert_eq!(
            order.equivalence.model_count,
            GeoResidualCountComparison::Matches
        );
        assert_eq!(
            order.equivalence.backbone,
            GeoResidualBackboneComparison::Matches
        );
        assert!(order.equivalence.truth_membership_matches);
        assert!(order.formula_comparable_to_search);
        assert!(!order.metrics_comparable_to_search);
        assert!(matches!(
            order.equivalence.answer_sets,
            GeoResidualAnswerSetComparison::NotMaterialized { .. }
        ));
    }
    assert!(
        grouped.final_serialized_node_count > interleaved.final_serialized_node_count * 8,
        "grouped={} interleaved={}",
        grouped.final_serialized_node_count,
        interleaved.final_serialized_node_count
    );
}

#[test]
fn corrupted_explicit_order_is_rejected() {
    let case = geo_residual_order_sensitivity_case(3);
    let mut variables = vec![
        GeoEntityRef::new(GeoEntityLevel::Parcel, "eq:p000"),
        GeoEntityRef::new(GeoEntityLevel::Parcel, "eq:p000"),
        GeoEntityRef::new(GeoEntityLevel::Building, "eq:b000"),
    ];
    variables.sort();
    let error = compile_geo_residual_obdd(
        &case.request,
        &GeoResidualVariableOrder::Explicit {
            name: "corrupt_duplicate_and_missing".to_string(),
            variables,
        },
    )
    .expect_err("corrupt order must fail");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);
}

#[test]
fn duplicate_case_ids_and_order_names_are_rejected() {
    let mut duplicate_case_input = small_input();
    duplicate_case_input.cases[1].case_id = duplicate_case_input.cases[0].case_id.clone();
    let error = run_geo_residual_benchmark(&duplicate_case_input)
        .expect_err("duplicate case ids must fail");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);

    let mut duplicate_order_input = small_input();
    duplicate_order_input.orders = vec![
        GeoResidualVariableOrder::Canonical,
        GeoResidualVariableOrder::Canonical,
    ];
    let error = run_geo_residual_benchmark(&duplicate_order_input)
        .expect_err("duplicate order names must fail");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);
}

#[test]
fn obdd_membership_rejects_noncanonical_unknown_or_duplicate_models() {
    let request = GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels: vec!["p0".to_string(), "p1".to_string()],
            buildings: Vec::new(),
        },
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 8,
        max_materialized_models: 8,
    };
    let obdd = compile_geo_residual_obdd(&request, &GeoResidualVariableOrder::Canonical)
        .expect("compile OBDD");
    let equivalence = verify_geo_residual_obdd(
        &request,
        &obdd,
        &[
            GeoCompositionModel {
                parcels: vec!["p1".to_string(), "p0".to_string()],
                buildings: Vec::new(),
            },
            GeoCompositionModel {
                parcels: vec!["p0".to_string(), "p0".to_string()],
                buildings: Vec::new(),
            },
            GeoCompositionModel {
                parcels: vec!["p9".to_string()],
                buildings: Vec::new(),
            },
        ],
        8,
    )
    .expect("verification runs");

    for row in equivalence.truth_membership {
        assert!(!row.request_membership);
        assert!(!row.obdd_membership);
    }
}

#[test]
fn answer_set_materialization_allows_exact_count_equal_to_cap() {
    let case = GeoResidualBenchmarkCase {
        case_id: "exact_answer_set_cap_boundary".to_string(),
        source: "unit_test_boundary_case".to_string(),
        shape_basis: GeoResidualShapeBasis::SyntheticOrderSensitivityControl,
        measurement_basis: "One satisfying model followed by unsatisfying masks in mask order."
            .to_string(),
        request: GeoCompositionRequest {
            version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
            profile: Default::default(),
            universe: GeoCompositionUniverse {
                parcels: vec!["p0".to_string()],
                buildings: vec![GeoBuildingCandidate {
                    id: "b0".to_string(),
                    parcel_ids: Vec::new(),
                }],
            },
            hard_constraints: vec![GeoHardConstraint {
                id: "forbid-b0".to_string(),
                constraint: GeoHardConstraintKind::Forbid {
                    member: GeoEntityRef::new(GeoEntityLevel::Building, "b0"),
                },
            }],
            soft_preferences: Vec::new(),
            max_assignments: 8,
            max_materialized_models: 8,
        },
        truth_models: vec![GeoCompositionModel {
            parcels: vec!["p0".to_string()],
            buildings: Vec::new(),
        }],
        orders: Vec::new(),
    };
    let input = GeoResidualBenchmarkInput {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: "bd-19wp-cap-boundary".to_string(),
        cases: vec![case],
        orders: vec![GeoResidualVariableOrder::Canonical],
        max_answer_set_models: 1,
    };

    let report = run_geo_residual_benchmark(&input).expect("benchmark runs");
    assert!(matches!(
        report.cases[0].orders[0].equivalence.answer_sets,
        GeoResidualAnswerSetComparison::Matches { model_count: 1 }
    ));
    assert!(report.cases[0].orders[0].metrics_comparable_to_search);
}

#[test]
fn terminal_root_reports_fixed_terminal_overhead_separately() {
    let case = GeoResidualBenchmarkCase {
        case_id: "terminal_false_fixed_terminal_overhead".to_string(),
        source: "unit_test_terminal_case".to_string(),
        shape_basis: GeoResidualShapeBasis::SyntheticOrderSensitivityControl,
        measurement_basis: "Unsatisfiable one-parcel case whose OBDD root is the false terminal."
            .to_string(),
        request: GeoCompositionRequest {
            version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
            profile: Default::default(),
            universe: GeoCompositionUniverse {
                parcels: vec!["p0".to_string()],
                buildings: Vec::new(),
            },
            hard_constraints: vec![GeoHardConstraint {
                id: "forbid-p0".to_string(),
                constraint: GeoHardConstraintKind::Forbid {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p0"),
                },
            }],
            soft_preferences: Vec::new(),
            max_assignments: 4,
            max_materialized_models: 4,
        },
        truth_models: Vec::new(),
        orders: Vec::new(),
    };
    let input = GeoResidualBenchmarkInput {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: "bd-19wp-terminal-overhead".to_string(),
        cases: vec![case],
        orders: vec![GeoResidualVariableOrder::Canonical],
        max_answer_set_models: 0,
    };

    let report = run_geo_residual_benchmark(&input).expect("benchmark runs");
    let order = &report.cases[0].orders[0];
    assert_eq!(order.final_serialized_node_count, 2);
    assert_eq!(order.root_reachable_node_count, 1);
    assert_eq!(order.fixed_terminal_overhead_node_count, 1);
}

#[test]
fn unreachable_appended_obdd_node_is_rejected_even_with_matching_digest() {
    let case = worked_case("case_6_dense_one_parcel_multi_building");
    let mut obdd = compile_geo_residual_obdd(&case.request, &GeoResidualVariableOrder::Canonical)
        .expect("compile OBDD");
    obdd.nodes.push(GeoResidualObddNode::Decision {
        variable: obdd.variables[0].clone(),
        low: 0,
        high: 1,
    });
    obdd.construction_arena_node_count = obdd.nodes.len();
    obdd.construction_arena_nonterminal_node_count = obdd.nodes.len() - 2;
    obdd.construction_peak_node_count = obdd.nodes.len();
    refresh_obdd_digest(&mut obdd);

    let error = verify_geo_residual_obdd(&case.request, &obdd, &case.truth_models, 512)
        .expect_err("unreachable nonterminal must fail");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);
    assert!(
        error.message.contains("unreachable nonterminal"),
        "{}",
        error.message
    );
}

#[test]
fn obdd_counter_mismatch_is_rejected() {
    let case = worked_case("case_6_dense_one_parcel_multi_building");
    let mut obdd = compile_geo_residual_obdd(&case.request, &GeoResidualVariableOrder::Canonical)
        .expect("compile OBDD");
    obdd.construction_arena_nonterminal_node_count += 1;

    let error = verify_geo_residual_obdd(&case.request, &obdd, &case.truth_models, 512)
        .expect_err("counter mismatch must fail");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);
    assert!(
        error.message.contains("nonterminal count"),
        "{}",
        error.message
    );
}

#[test]
fn corrupted_obdd_artifact_is_detected_as_inequivalent() {
    let case = worked_case("case_6_dense_one_parcel_multi_building");
    let mut obdd = compile_geo_residual_obdd(&case.request, &GeoResidualVariableOrder::Canonical)
        .expect("compile OBDD");
    obdd.root = 1;

    let error = verify_geo_residual_obdd(&case.request, &obdd, &case.truth_models, 512)
        .expect_err("digest mismatch catches direct root tampering");
    assert_eq!(error.code, GeoResidualBenchmarkErrorCode::InvalidInput);
}
