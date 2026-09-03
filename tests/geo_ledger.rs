#![forbid(unsafe_code)]

mod geo {
    pub use canon::geo::*;
}

#[allow(dead_code)]
#[path = "../src/geo/ledger.rs"]
mod ledger;

use canon::geo::{
    compile_evidence, GeoCandidateReachStatus, GeoCompositionArtifact, GeoCompositionBackbone,
    GeoCompositionFallback, GeoCompositionModel, GeoCompositionProfile, GeoCompositionStatus,
    GeoCompositionSummary, GeoEvidenceCompilationRequest, GeoLabeledCompositionCase,
    GeoModelCountScope, GeoPopulationEvaluationRequest, GeoTruthPlane,
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_COMPOSITION_VERSION,
};
use ledger::{
    build_collateral_ledger, build_ledger_row, canonical_collateral_ledger_bytes, roll_up_deal,
    validate_ledger, GeoCollateralLedger, GeoCollateralLedgerProofClass, GeoLedgerErrorCode,
    GeoLedgerLoanRef, GeoLedgerRow, GeoSourceReleasePin, CANON_GEO_COLLATERAL_LEDGER_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const E4_RESTACK_EVALUATION: &str =
    "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json";
const E4_RESTACK_EVALUATION_BYTES: &[u8] = include_bytes!(
    "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json"
);
const E4_POPULATION_REQUEST: &str = "../tests/fixtures/geo/e4_gate_v2_population_request.json";
const E4_ENRICHMENT: &str = "../tests/fixtures/geo/e4_gate_v2_evidence_enrichment.json";

const SYNTHETIC_ACCESSION: &str = "0000000000-26-000001";
const SYNTHETIC_DEAL: &str = "fixture-deal-a";
const FORCED_REACH_NONE_CASE: &str = "3cf11e9a58e3b710";
const FORCED_REACH_NONE_LOAN: &str = "073ad3a0862827c75501ac66570eb783";
const FORCED_REACH_NONE_REASON: &str = "no_candidate_parcels";

#[test]
fn t07_fixture_gate_rows_roll_up_per_truth_plane_without_total() {
    let ledger = fixture_ledger();

    assert_eq!(ledger.rows.len(), 15);
    assert_eq!(ledger.rollups.len(), 1);
    assert_eq!(ledger.rollups[0].rows, 15);
    assert_eq!(ledger.rollups[0].deal_id, SYNTHETIC_DEAL);
    assert_eq!(ledger.rollups[0].accession, SYNTHETIC_ACCESSION);
    assert_eq!(
        ledger
            .rollups
            .iter()
            .flat_map(|rollup| rollup.truth_planes.values())
            .map(|counts| counts.resolved
                + counts.ambiguous
                + counts.conflict
                + counts.reach_none
                + counts.budget_fallback)
            .sum::<u64>(),
        ledger.rollups[0].rows
    );
    let gate_counts = ledger.rollups[0]
        .truth_planes
        .get(&GeoTruthPlane::GateV2Historical)
        .expect("gate truth-plane counts");
    assert_eq!(gate_counts.resolved, 4);
    assert_eq!(gate_counts.ambiguous, 9);
    assert_eq!(gate_counts.conflict, 0);
    assert_eq!(gate_counts.reach_none, 1);
    assert_eq!(gate_counts.budget_fallback, 1);

    let serialized_rollup = serde_json::to_value(&ledger.rollups[0]).expect("rollup JSON");
    assert!(serialized_rollup.get("total").is_none());
    let top_level_numeric_keys = serialized_rollup
        .as_object()
        .expect("rollup object")
        .iter()
        .filter_map(|(key, value)| value.is_number().then_some(key.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(top_level_numeric_keys, vec!["rows"]);

    let reach_none_rows = ledger
        .rows
        .iter()
        .filter(|row| row.reach == GeoCandidateReachStatus::None)
        .collect::<Vec<_>>();
    assert_eq!(reach_none_rows.len(), 1);
    let row = reach_none_rows[0];
    assert_eq!(row.loan_id, FORCED_REACH_NONE_LOAN);
    assert_eq!(
        row.reach_none_reason.as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    assert_eq!(row.parcel_set, None);
    assert_eq!(row.building_set, None);

    assert!(ledger.rows.iter().any(|row| {
        row.composition_status == GeoCompositionStatus::Ambiguous
            && row.count_exact
            && !row.ambiguous_parcel_set.is_empty()
    }));

    let recomputed = roll_up_deal(&ledger.rows).expect("recomputed rollup");
    assert_eq!(
        serde_json::to_vec(&recomputed).expect("recomputed rollup bytes"),
        serde_json::to_vec(&ledger.rollups[0]).expect("stored rollup bytes")
    );
    validate_ledger(&ledger).expect("ledger validates");

    let mut unlabeled_rows = ledger.rows[0..2].to_vec();
    unlabeled_rows[0].truth_plane = None;
    unlabeled_rows[1].truth_plane = Some(GeoTruthPlane::RoundExactLenderParty);
    let error = roll_up_deal(&unlabeled_rows).expect_err("unlabeled plane refuses");
    assert_eq!(error.code, GeoLedgerErrorCode::LedgerTruthPlanePooled);
    assert_eq!(error.detail["field"], "truth_plane");
}

#[test]
fn t23_build_ledger_row_refuses_fabricated_sets_without_artifacts() {
    let loan = fixture_loan("loan-a");
    let evidence_request = sample_evidence_request();
    let evidence = compile_evidence(&evidence_request).expect("sample evidence compiles");
    let composition = sample_composition(GeoCompositionStatus::Resolved, vec!["p1"], 1);
    let pins = vec![fixture_pin()];

    let missing_composition = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        None,
        Some(&evidence),
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("missing composition refuses");
    assert_eq!(
        missing_composition.code,
        GeoLedgerErrorCode::LedgerSetsWithoutArtifacts
    );
    assert_eq!(missing_composition.detail["field"], "composition");

    let missing_evidence = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        Some(&composition),
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("missing evidence refuses");
    assert_eq!(
        missing_evidence.code,
        GeoLedgerErrorCode::LedgerSetsWithoutArtifacts
    );
    assert_eq!(missing_evidence.detail["field"], "evidence");

    let missing_reason = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::None,
        None,
        None,
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("reach none without reason refuses");
    assert_eq!(missing_reason.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(missing_reason.detail["field"], "reach_none_reason");

    let abstained = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::None,
        Some(FORCED_REACH_NONE_REASON.to_string()),
        None,
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect("reach none with reason emits a row");
    assert_eq!(abstained.reach, GeoCandidateReachStatus::None);
    assert_eq!(
        abstained.reach_none_reason.as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    assert_eq!(abstained.parcel_set, None);
    assert_eq!(abstained.building_set, None);
}

#[test]
fn t26_fixture_pins_cannot_be_relabelled_as_live() {
    let ledger = fixture_ledger();
    validate_ledger(&ledger).expect("fixture ledger validates with fixture pins");

    let mut relabeled = ledger.clone();
    relabeled.rows[0].source_release_pins[0].source_dataset = "nyc.mappluto.26v1".to_string();
    let error = validate_ledger(&relabeled).expect_err("relabeled fixture pin refuses");
    assert_eq!(error.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "source_release_pins");
    assert_eq!(error.detail["source_dataset"], "nyc.mappluto.26v1");
    assert_eq!(error.detail["loan_id"], relabeled.rows[0].loan_id);

    let mut additive_row_value = serde_json::to_value(&ledger.rows[0]).expect("row JSON");
    additive_row_value["building_last_observed"] = json!([]);
    let additive_row: GeoLedgerRow =
        serde_json::from_value(additive_row_value).expect("row accepts additive fields");
    let mut additive_ledger = ledger.clone();
    additive_ledger.rows[0] = additive_row;
    additive_ledger.rollups = vec![roll_up_deal(&additive_ledger.rows).expect("rollup")];
    validate_ledger(&additive_ledger).expect("additive future row fields do not refuse");
}

#[test]
fn t27_ledger_module_has_no_fixture_or_solver_literals() {
    let source = std::fs::read_to_string("src/geo/ledger.rs").expect("ledger source");
    let lower = source.to_ascii_lowercase();
    for forbidden in [
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
        "franklin",
        "solve_composition",
        "openai",
        "anthropic",
        "gemini",
    ] {
        assert!(
            !lower.contains(&forbidden.to_ascii_lowercase()),
            "generic ledger module contains forbidden literal {forbidden}"
        );
    }
}

#[test]
fn collateral_ledger_schema_matches_a_real_instance() {
    let schema: Value = serde_json::from_str(include_str!(
        "../schemas/canon.geo.collateral_ledger.v0.schema.json"
    ))
    .expect("schema parses");
    assert_eq!(schema["title"], "canon.geo.collateral_ledger.v0");
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_GEO_COLLATERAL_LEDGER_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);

    let ledger = fixture_ledger();
    let instance = serde_json::to_value(&ledger).expect("ledger JSON");
    assert_eq!(instance["version"], CANON_GEO_COLLATERAL_LEDGER_VERSION);
    assert!(instance["rows"].as_array().expect("rows").len() == 15);
    assert!(canonical_collateral_ledger_bytes(&ledger)
        .expect("canonical ledger bytes")
        .starts_with(b"{\"version\":\"canon_geo_collateral_ledger.v0\""));
}

fn fixture_ledger() -> GeoCollateralLedger {
    let eval = load_restack_evaluation();
    let population = load_population_request();
    let loan_keys = load_loan_keys();
    let population_by_case = population
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();
    for case in &eval.cases {
        let population_case = population_by_case
            .get(case.case_id.as_str())
            .expect("population case");
        let loan = GeoLedgerLoanRef {
            accession: SYNTHETIC_ACCESSION.to_string(),
            deal_id: SYNTHETIC_DEAL.to_string(),
            loan_id: loan_keys
                .get(case.case_id.as_str())
                .expect("loan key")
                .clone(),
            deed_ids: Vec::new(),
        };
        let reach = if case.case_id == FORCED_REACH_NONE_CASE {
            GeoCandidateReachStatus::None
        } else {
            match case.candidate_reach.as_str() {
                "full" => GeoCandidateReachStatus::Full,
                "partial" | "none" => GeoCandidateReachStatus::Partial,
                other => panic!("unknown candidate reach {other}"),
            }
        };
        let reach_none_reason =
            (reach == GeoCandidateReachStatus::None).then(|| FORCED_REACH_NONE_REASON.to_string());
        let composition = composition_from_restack_case(case, population_case);
        let evidence =
            compile_evidence(&population_case.evidence).expect("compile fixture evidence");
        rows.push(
            build_ledger_row(
                &loan,
                reach,
                reach_none_reason,
                Some(&composition),
                Some(&evidence),
                Some(population_case.truth_plane),
                &[fixture_pin()],
            )
            .expect("build ledger row"),
        );
    }
    let ledger =
        build_collateral_ledger(rows, GeoCollateralLedgerProofClass::Fixture).expect("ledger");
    assert_eq!(
        ledger
            .rows
            .iter()
            .find(|row| row.loan_id == FORCED_REACH_NONE_LOAN)
            .expect("forced row")
            .reach,
        GeoCandidateReachStatus::None
    );
    ledger
}

fn composition_from_restack_case(
    case: &RestackCaseEvaluation,
    population_case: &GeoLabeledCompositionCase,
) -> GeoCompositionArtifact {
    let status = composition_status(&case.status);
    let hard_forced = GeoCompositionBackbone {
        parcels: sorted_unique(case.hard_forced.parcels.clone()),
        buildings: sorted_unique(case.hard_forced.buildings.clone()),
    };
    let residual_models = residual_models_for_case(status, &hard_forced, population_case);
    GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        evidence_compilation: None,
        status,
        resolved_claim: None,
        summary: GeoCompositionSummary {
            parcel_candidates: case.candidate_members,
            building_candidates: 0,
            candidate_assignments: case
                .residual_model_count
                .unwrap_or(case.candidate_members as u64),
            candidate_assignments_saturated: case.residual_count_saturated,
            structurally_feasible_assignments: case.residual_model_count.unwrap_or_default(),
            structurally_feasible_assignments_complete: case.residual_count_complete,
            structurally_feasible_assignments_saturated: case.residual_count_saturated,
            hard_constraint_evaluations: 0,
            hard_constraint_evaluations_complete: case.status != "component_budget_fallback",
            hard_constraint_evaluations_saturated: false,
            residual_model_count: case.residual_model_count.unwrap_or_default(),
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: case.residual_count_complete,
            residual_model_count_saturated: case.residual_count_saturated,
            summary_counts_saturated: case.residual_count_saturated,
            component_count: usize::from(status != GeoCompositionStatus::BudgetFallback),
            residual_models_materialized: !residual_models.is_empty(),
        },
        hard_forced,
        backbone_complete: case.backbone_complete,
        factorization: Vec::new(),
        residual_models,
        soft_ranked: Vec::new(),
        conflict_constraint_ids: Vec::new(),
        conflict_core_complete: None,
        budget_fallback: (status == GeoCompositionStatus::BudgetFallback).then(|| {
            GeoCompositionFallback {
                component_keys: Vec::new(),
                max_component_variables: case.candidate_members,
                configured_max_assignments: 0,
                guidance: "fixture restack receipt reached component-budget fallback".to_string(),
            }
        }),
        entity_projection: None,
    }
}

fn residual_models_for_case(
    status: GeoCompositionStatus,
    hard_forced: &GeoCompositionBackbone,
    population_case: &GeoLabeledCompositionCase,
) -> Vec<GeoCompositionModel> {
    match status {
        GeoCompositionStatus::Resolved => vec![GeoCompositionModel {
            parcels: hard_forced.parcels.clone(),
            buildings: hard_forced.buildings.clone(),
        }],
        GeoCompositionStatus::Ambiguous => {
            let candidates = population_case
                .evidence
                .universe
                .parcels
                .iter()
                .filter(|parcel| !hard_forced.parcels.binary_search(parcel).is_ok())
                .take(2)
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                candidates.len() >= 2,
                "fixture ambiguous rows need at least two candidate parcels"
            );
            candidates
                .into_iter()
                .map(|candidate| {
                    let mut parcels = hard_forced.parcels.clone();
                    parcels.push(candidate);
                    parcels = sorted_unique(parcels);
                    GeoCompositionModel {
                        parcels,
                        buildings: hard_forced.buildings.clone(),
                    }
                })
                .collect()
        }
        GeoCompositionStatus::Conflict | GeoCompositionStatus::BudgetFallback => Vec::new(),
    }
}

fn composition_status(status: &str) -> GeoCompositionStatus {
    match status {
        "resolved" => GeoCompositionStatus::Resolved,
        "ambiguous" => GeoCompositionStatus::Ambiguous,
        "conflict" => GeoCompositionStatus::Conflict,
        "component_budget_fallback" => GeoCompositionStatus::BudgetFallback,
        other => panic!("unknown restack status {other}"),
    }
}

fn sample_evidence_request() -> GeoEvidenceCompilationRequest {
    load_population_request().cases[0].evidence.clone()
}

fn sample_composition(
    status: GeoCompositionStatus,
    parcel_ids: Vec<&str>,
    residual_model_count: u64,
) -> GeoCompositionArtifact {
    GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        evidence_compilation: None,
        status,
        resolved_claim: None,
        summary: GeoCompositionSummary {
            parcel_candidates: parcel_ids.len(),
            building_candidates: 0,
            candidate_assignments: residual_model_count,
            candidate_assignments_saturated: false,
            structurally_feasible_assignments: residual_model_count,
            structurally_feasible_assignments_complete: true,
            structurally_feasible_assignments_saturated: false,
            hard_constraint_evaluations: residual_model_count,
            hard_constraint_evaluations_complete: true,
            hard_constraint_evaluations_saturated: false,
            residual_model_count,
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: true,
            residual_model_count_saturated: false,
            summary_counts_saturated: false,
            component_count: 1,
            residual_models_materialized: true,
        },
        hard_forced: GeoCompositionBackbone {
            parcels: parcel_ids.iter().map(|parcel| parcel.to_string()).collect(),
            buildings: Vec::new(),
        },
        backbone_complete: true,
        factorization: Vec::new(),
        residual_models: vec![GeoCompositionModel {
            parcels: parcel_ids.iter().map(|parcel| parcel.to_string()).collect(),
            buildings: Vec::new(),
        }],
        soft_ranked: Vec::new(),
        conflict_constraint_ids: Vec::new(),
        conflict_core_complete: None,
        budget_fallback: None,
        entity_projection: None,
    }
}

fn fixture_loan(loan_id: &str) -> GeoLedgerLoanRef {
    GeoLedgerLoanRef {
        accession: SYNTHETIC_ACCESSION.to_string(),
        deal_id: SYNTHETIC_DEAL.to_string(),
        loan_id: loan_id.to_string(),
        deed_ids: Vec::new(),
    }
}

fn fixture_pin() -> GeoSourceReleasePin {
    GeoSourceReleasePin {
        source_dataset: "fixture.e4_gate_v2_restack".to_string(),
        source_release: "2026-09-03".to_string(),
        blake3: format!(
            "blake3:{}",
            blake3::hash(E4_RESTACK_EVALUATION_BYTES).to_hex()
        ),
    }
}

fn load_restack_evaluation() -> RestackEvaluation {
    serde_json::from_str(include_str!(
        "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json"
    ))
    .expect(E4_RESTACK_EVALUATION)
}

fn load_population_request() -> GeoPopulationEvaluationRequest {
    serde_json::from_str(include_str!(
        "../tests/fixtures/geo/e4_gate_v2_population_request.json"
    ))
    .expect(E4_POPULATION_REQUEST)
}

fn load_loan_keys() -> BTreeMap<String, String> {
    let enrichment: EvidenceEnrichment = serde_json::from_str(include_str!(
        "../tests/fixtures/geo/e4_gate_v2_evidence_enrichment.json"
    ))
    .expect(E4_ENRICHMENT);
    enrichment
        .cases
        .into_iter()
        .map(|case| (case.case_id, case.loan_key))
        .collect()
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Deserialize)]
struct RestackEvaluation {
    cases: Vec<RestackCaseEvaluation>,
}

#[derive(Debug, Deserialize)]
struct RestackCaseEvaluation {
    case_id: String,
    status: String,
    candidate_reach: String,
    candidate_members: usize,
    residual_model_count: Option<u64>,
    residual_count_complete: bool,
    residual_count_saturated: bool,
    hard_forced: RestackForcedSet,
    backbone_complete: bool,
}

#[derive(Debug, Deserialize)]
struct RestackForcedSet {
    parcels: Vec<String>,
    buildings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvidenceEnrichment {
    cases: Vec<EvidenceEnrichmentCase>,
}

#[derive(Debug, Deserialize)]
struct EvidenceEnrichmentCase {
    case_id: String,
    loan_key: String,
}
