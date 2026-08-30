#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_RESIDUAL_BENCHMARK_VERSION, GeoCompositionModel, GeoCompositionRequest,
    GeoResidualBenchmarkCase, GeoResidualBenchmarkInput, GeoResidualShapeBasis,
    GeoResidualVariableOrder, geo_residual_measured_star_case, geo_residual_order_sensitivity_case,
    geo_residual_raw_observation_stress_case, run_geo_residual_benchmark,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
}

fn worked_case(case_id: &str, truth: GeoCompositionModel) -> GeoResidualBenchmarkCase {
    let corpus: WorkedCorpus =
        serde_json::from_str(include_str!("../tests/fixtures/geo/e4_worked_cases.json"))
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
        measurement_basis:
            "Appendix I worked corpus; retained real query-selected composition case.".to_string(),
        request: case.request,
        truth_models: vec![truth],
        orders: Vec::new(),
    }
}

fn benchmark_input() -> GeoResidualBenchmarkInput {
    GeoResidualBenchmarkInput {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: "bd-19wp-geo-residual-representations".to_string(),
        cases: vec![
            worked_case(
                "case_4_chimera_multi_street",
                GeoCompositionModel {
                    parcels: vec![
                        "1004540041".to_string(),
                        "1004540042".to_string(),
                        "1004540043".to_string(),
                        "1004540044".to_string(),
                        "1004540045".to_string(),
                        "1004540046".to_string(),
                    ],
                    buildings: vec![
                        "1006494".to_string(),
                        "1006495".to_string(),
                        "1006496".to_string(),
                        "1006497".to_string(),
                        "1006498".to_string(),
                        "1006499".to_string(),
                    ],
                },
            ),
            worked_case(
                "case_6_dense_one_parcel_multi_building",
                GeoCompositionModel {
                    parcels: vec!["1014477501".to_string()],
                    buildings: vec!["1076314".to_string()],
                },
            ),
            geo_residual_measured_star_case(
                "d11_brooklyn_dense_r9_component_width_5",
                "scripts/geo_measurements/README.md D.11: Brooklyn dense r9 max incidence component 5; source-bound MapPLUTO geom-v3 and NYC footprints.",
                5,
                64,
            ),
            geo_residual_measured_star_case(
                "strata_fema_bronx_merged_component_width_19",
                "docs/geo_design_session/STRATA_FEMA_BD3UN6.md F.3 retained mixed-contract merged Bronx max component 19; historical measurement, not canonical multi-source proof.",
                19,
                1_000_000,
            ),
            geo_residual_measured_star_case(
                "d11_staten_island_r9_component_width_65",
                "scripts/geo_measurements/README.md D.11: Staten Island low r9 max majority-incidence component 65; final solver width remains open.",
                65,
                4_096,
            ),
            geo_residual_raw_observation_stress_case(
                "f6_overture_r9_raw_star_width_118",
                "scripts/geo_measurements/README.md F.6: Overture+NYC raw observation parcel-star max 118 at r9; raw source-row stress upper bound, not latent solver-component width.",
                118,
                4_096,
            ),
            geo_residual_raw_observation_stress_case(
                "f6_overture_r8_raw_star_width_128",
                "scripts/geo_measurements/README.md F.6: Overture+NYC raw observation parcel-star max 128 at r8; raw source-row stress upper bound, not latent solver-component width.",
                128,
                4_096,
            ),
            geo_residual_order_sensitivity_case(12),
        ],
        orders: vec![
            GeoResidualVariableOrder::Canonical,
            GeoResidualVariableOrder::BuildingsFirst,
            GeoResidualVariableOrder::IncidenceInterleaved,
        ],
        max_answer_set_models: 4_096,
    }
}

#[test]
fn geo_residual_representations_smoke_equivalence() {
    let mut input = benchmark_input();
    input.cases.truncate(3);
    input.orders = vec![
        GeoResidualVariableOrder::Canonical,
        GeoResidualVariableOrder::IncidenceInterleaved,
    ];
    input.max_answer_set_models = 4_096;
    let report = run_geo_residual_benchmark(&input).expect("benchmark report");
    assert_eq!(report.cases.len(), 3);
    assert!(
        report
            .cases
            .iter()
            .flat_map(|case| case.orders.iter())
            .all(|order| order.equivalence.request_digest_matches)
    );
}

#[test]
#[ignore = "records local wall-time and OBDD size metrics for bd-19wp"]
fn geo_residual_representations_metrics_tier() {
    let report = run_geo_residual_benchmark(&benchmark_input()).expect("benchmark report");
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}
