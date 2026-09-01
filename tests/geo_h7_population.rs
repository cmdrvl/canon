#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_H7_ACRIS_RELEASE_DT, CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION,
    CANON_GEO_H7_BRIDGE_BUILD_ID, CANON_GEO_H7_COLLATERAL_SCOPE,
    CANON_GEO_H7_FROZEN_E4_ACCEPTANCE_CASES, CANON_GEO_H7_LENDER_MATCH_TRANSFORM,
    CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
    CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE, CANON_GEO_H7_POPULATION_ROWS_VERSION,
    CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE,
    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS, CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE,
    GeoEvidenceRecordRef, GeoH7AssociationPlane, GeoH7BoroughEdge, GeoH7CandidateReachStatus,
    GeoH7EmpiricalDiscrepancy, GeoH7EmpiricalDiscrepancyStatus, GeoH7ExternalReceiptKind,
    GeoH7ExternalReceiptRef, GeoH7FiledCountyMapping, GeoH7MapplutoReleasePin,
    GeoH7PlaneDenominator, GeoH7PopulationProvenance, GeoH7PopulationRowsRequest,
    GeoH7PopulationScope, GeoH7PopulationWarehouseRow, GeoH7QueryDisposition, GeoH7QueryReceipt,
    GeoH7ResultMode, GeoH7SourceEvidenceRecord, GeoH7SourceHash, GeoH7SourceRecordRole,
    GeoH7TruthPlaneSummary, GeoTruthPlane, canonical_h7_population_bytes, evaluate_population,
    materialize_h7_population_rows,
};
use serde_json::Value;

const H7_POPULATION_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_population_rows.v0.schema.json");
const H7_POPULATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_population.v0.schema.json");
const H7_STAGE2_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_master_party_candidates.sql");
const H7_STAGE3_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_multi_parcel_legal_residual.sql");
const H7_DENOMINATOR_CONTROL_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_denominator_control.sql");
const H7_ACCEPTED_TRUTH_EXPORT_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_truth_export.sql");
const H7_HALO_REACH_CONTROL_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_halo_reach_control.sql");
const H7_PIP_BLOCK_REACH_CONTROL_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_pip_block_reach_control.sql");
const H7_INCIDENCE_SHARD_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_incidence_shard.sql");
const SYNTHETIC_CURRENT_BRIDGE_BUILD_ID: &str = "bridge_build_current_snapshot_20260901_a";

fn base_request() -> GeoH7PopulationRowsRequest {
    GeoH7PopulationRowsRequest {
        version: CANON_GEO_H7_POPULATION_ROWS_VERSION.to_string(),
        population_scope: GeoH7PopulationScope::FixtureSubset,
        provenance: GeoH7PopulationProvenance {
            result_mode: GeoH7ResultMode::Replay,
            as_of: "2026-08-30T00:00:00Z".to_string(),
            acris_release_dt: CANON_GEO_H7_ACRIS_RELEASE_DT.to_string(),
            bridge_build_id: CANON_GEO_H7_BRIDGE_BUILD_ID.to_string(),
            collateral_scope: CANON_GEO_H7_COLLATERAL_SCOPE.to_string(),
            mappluto_releases: vec![mappluto_pin("26v2"), mappluto_pin("26v1")],
            primary_candidate_release: mappluto_pin(CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE),
            amount_cents_quantization: CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION.to_string(),
            round_amount_lattice_cents: CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS,
            lender_match_transform: CANON_GEO_H7_LENDER_MATCH_TRANSFORM.to_string(),
            filed_county_mapping: filed_county_mapping(),
            source_hashes: Vec::new(),
            query_receipts: vec![
                receipt("diagnostic_unfiltered_raw_filed_county_3016", 3),
                receipt("diagnostic_geocoder_county_fips_647_2291", 3),
                receipt("fixture_raw_property_state_ny_control_653_2321", 7),
                receipt("diagnostic_originator_availability_discrepancy", 2),
                receipt("diagnostic_round_lender_candidate_aggregation_drift", 1),
                cancelled_receipt("cancelled_round_lender_legal_residual"),
            ],
            external_receipts: vec![
                external_receipt(
                    "archived_appendix_g7_h7_originator_availability",
                    GeoH7ExternalReceiptKind::ArchivedAppendixG7,
                    "archived G7 originator availability baseline",
                ),
                external_receipt(
                    "archived_appendix_g7_h7_round_candidate_aggregation",
                    GeoH7ExternalReceiptKind::ArchivedAppendixG7,
                    "archived G7 round candidate aggregation baseline",
                ),
                external_receipt(
                    "1385b1fd64bf266f",
                    GeoH7ExternalReceiptKind::RevealLineage,
                    "ORIGINATORNAME lineage receipt",
                ),
                external_receipt(
                    "dbd7d7dbc84727b2",
                    GeoH7ExternalReceiptKind::RevealLineage,
                    "ORIGINATOR_MATCH_TEXT transform receipt",
                ),
                external_receipt(
                    "01c6bd19-0821-9afc-006c-c703088c0936",
                    GeoH7ExternalReceiptKind::WarehouseQueryHistory,
                    "fresh originator availability discrepancy query",
                ),
                external_receipt(
                    "01c6bd25-0821-a0dc-006c-c703088c27be",
                    GeoH7ExternalReceiptKind::WarehouseQueryHistory,
                    "fresh round candidate aggregation discrepancy query",
                ),
                external_receipt(
                    "01c6bd28-0821-a0dc-006c-c703088c27c6",
                    GeoH7ExternalReceiptKind::WarehouseQueryHistory,
                    "cancelled round legal residual query",
                ),
            ],
            empirical_discrepancies: vec![
                GeoH7EmpiricalDiscrepancy {
                    subject: "originator_availability_by_truth_plane".to_string(),
                    archived_measurement: "G7 archived availability: non-round 605/653, round 2173/2321".to_string(),
                    fresh_measurement: "2026-08-30 control 01c6bd19-0821-9afc-006c-c703088c0936: non-round 653/653, round 2317/2321; no ambiguities".to_string(),
                    status: GeoH7EmpiricalDiscrepancyStatus::Open,
                    receipt_ids: vec![
                        "archived_appendix_g7_h7_originator_availability".to_string(),
                        "1385b1fd64bf266f".to_string(),
                        "dbd7d7dbc84727b2".to_string(),
                        "01c6bd19-0821-9afc-006c-c703088c0936".to_string(),
                    ],
                },
                GeoH7EmpiricalDiscrepancy {
                    subject: "round_lender_candidate_aggregation".to_string(),
                    archived_measurement: "G7 archived candidate aggregation: 2173 round loans with exact originator, 182 candidate loans, 277 loan-document pairs".to_string(),
                    fresh_measurement: "2026-08-30 query 01c6bd25-0821-a0dc-006c-c703088c27be: 2317 round loans with exact originator, 311 candidate loans, 439 loan-document pairs; legal residual query 01c6bd28-0821-a0dc-006c-c703088c27c6 cancelled".to_string(),
                    status: GeoH7EmpiricalDiscrepancyStatus::Open,
                    receipt_ids: vec![
                        "archived_appendix_g7_h7_round_candidate_aggregation".to_string(),
                        "01c6bd25-0821-a0dc-006c-c703088c27be".to_string(),
                        "01c6bd28-0821-a0dc-006c-c703088c27c6".to_string(),
                    ],
                },
            ],
            row_cap: 10,
            observed_rows: 0,
            observed_payload_blake3: None,
        },
        plane_denominators: vec![
            GeoH7PlaneDenominator {
                truth_plane: GeoTruthPlane::NonRoundAmountDateLegalBorough,
                eligible_loans: 653,
                candidate_loans: 262,
                legal_confirmed_candidate_loans: 221,
                accepted_loans: 172,
                ambiguous_loans: 49,
                candidate_no_legal_confirmation_loans: 41,
                no_candidate_loans: 391,
                selected_multi_parcel_loans: 1,
            },
            GeoH7PlaneDenominator {
                truth_plane: GeoTruthPlane::RoundExactLenderParty,
                eligible_loans: 2321,
                candidate_loans: 182,
                legal_confirmed_candidate_loans: 179,
                accepted_loans: 149,
                ambiguous_loans: 30,
                candidate_no_legal_confirmation_loans: 3,
                no_candidate_loans: 2139,
                selected_multi_parcel_loans: 1,
            },
        ],
        rows: vec![
            non_round_row("loan-nonround", "doc-nonround", "26v1"),
            non_round_row("loan-nonround", "doc-nonround", "26v2"),
            round_row("loan-round", "doc-round", "26v1"),
            round_row("loan-round", "doc-round", "26v2"),
        ],
        max_cases: 8,
        max_assignments: 64,
        max_materialized_models: 64,
    }
}

fn synthetic_complete_shape_request() -> GeoH7PopulationRowsRequest {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::RetainedComplete;
    request.plane_denominators[0].selected_multi_parcel_loans = 35;
    request.plane_denominators[1].selected_multi_parcel_loans = 14;
    request
}

fn synthetic_current_build_live_complete_request() -> GeoH7PopulationRowsRequest {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::LiveComplete;
    request.provenance.result_mode = GeoH7ResultMode::Live;
    request.provenance.as_of = "2026-09-01T00:00:00Z".to_string();
    request.provenance.bridge_build_id = SYNTHETIC_CURRENT_BRIDGE_BUILD_ID.to_string();
    request.provenance.source_hashes = vec![
        GeoH7SourceHash {
            source: "synthetic_current.loan_property_bridge".to_string(),
            hash_kind: "test_sha256".to_string(),
            sha256: "1".repeat(64),
        },
        GeoH7SourceHash {
            source: "synthetic_current.acris_master".to_string(),
            hash_kind: "test_sha256".to_string(),
            sha256: "2".repeat(64),
        },
        GeoH7SourceHash {
            source: "synthetic_current.acris_legal".to_string(),
            hash_kind: "test_sha256".to_string(),
            sha256: "3".repeat(64),
        },
        GeoH7SourceHash {
            source: "synthetic_current.acris_party".to_string(),
            hash_kind: "test_sha256".to_string(),
            sha256: "4".repeat(64),
        },
        GeoH7SourceHash {
            source: "synthetic_current.mappluto_pins".to_string(),
            hash_kind: "test_sha256".to_string(),
            sha256: "5".repeat(64),
        },
    ];
    install_live_query_receipts(&mut request);
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::NonRoundAmountDateLegalBorough,
            1,
        ));
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::RoundExactLenderParty,
            1,
        ));
    retag_bridge_source_records(&mut request, SYNTHETIC_CURRENT_BRIDGE_BUILD_ID);
    request.provenance.observed_rows = request.rows.len() as u64;
    request.provenance.row_cap = 100;
    request
}

#[test]
fn materializes_replay_population_without_pooling_truth_planes_or_candidate_releases() {
    let request = base_request();
    let artifact = materialize_h7_population_rows(&request).expect("h7 replay materializes");

    assert_eq!(artifact.version, CANON_GEO_H7_POPULATION_VERSION);
    assert_eq!(artifact.summary.source_rows, 4);
    assert_eq!(artifact.summary.materialized_case_rows, 4);
    assert_eq!(artifact.summary.materialized_unique_accepted_loans, 2);
    assert_eq!(artifact.summary.solver_population_subjects, 2);
    assert_eq!(
        artifact.summary.population_scope,
        GeoH7PopulationScope::FixtureSubset
    );
    assert_eq!(artifact.population.cases.len(), 2);
    assert_eq!(artifact.summary.truth_planes.len(), 2);
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .candidate_loans,
        262
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .legal_confirmed_candidate_loans,
        221
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .candidate_no_legal_confirmation_loans,
        41
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .selected_multi_parcel_loans,
        1
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::RoundExactLenderParty
        )
        .candidate_loans,
        182
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::RoundExactLenderParty
        )
        .no_candidate_loans,
        2139
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::RoundExactLenderParty
        )
        .selected_multi_parcel_loans,
        1
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .materialized_case_rows,
        2
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .materialized_unique_accepted_loans,
        1
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::RoundExactLenderParty
        )
        .materialized_case_rows,
        2
    );
    assert_eq!(artifact.summary.strata.len(), 4);
    assert!(
        artifact
            .cases
            .iter()
            .any(|case| case.candidate_release.release == "26v1")
    );
    assert!(
        artifact
            .cases
            .iter()
            .any(|case| case.candidate_release.release == "26v2")
    );
    assert!(
        artifact
            .population
            .cases
            .iter()
            .all(|case| case.id.starts_with("h7-subject:")),
        "evaluation population must use loan-grain subject ids"
    );

    let partial_case = artifact
        .population
        .cases
        .iter()
        .find(|case| case.evidence.universe.parcels == vec!["1000000001".to_string()])
        .expect("partial-reach candidate-only universe");
    assert_eq!(
        partial_case.truth.parcels,
        vec!["1000000001".to_string(), "1000000002".to_string()]
    );
    assert!(
        !partial_case
            .evidence
            .universe
            .parcels
            .contains(&"1000000002".to_string()),
        "truth parcel must not be injected into solver candidate evidence"
    );

    let evaluation = evaluate_population(&artifact.population).expect("population evaluates");
    assert_eq!(evaluation.summary.cases, 2);
    assert_eq!(evaluation.summary.candidate_reach_partial_cases, 2);

    let bytes = canonical_h7_population_bytes(&artifact).expect("canonical bytes");
    let replay = canonical_h7_population_bytes(&artifact).expect("canonical bytes replay");
    assert_eq!(bytes, replay);

    let mut reordered_request = base_request();
    reordered_request.provenance.external_receipts.reverse();
    let reordered_artifact = materialize_h7_population_rows(&reordered_request)
        .expect("external receipt input order must not change materialization");
    assert_eq!(artifact.provenance, reordered_artifact.provenance);
    assert_eq!(
        bytes,
        canonical_h7_population_bytes(&reordered_artifact)
            .expect("reordered external receipts serialize canonically")
    );
}

#[test]
fn fixture_subset_accepts_nonhistorical_declared_bridge_snapshot() {
    let mut request = base_request();
    let fixture_build_id = "fixture_subset_snapshot_20260901_a";
    request.provenance.bridge_build_id = fixture_build_id.to_string();
    retag_bridge_source_records(&mut request, fixture_build_id);

    let artifact = materialize_h7_population_rows(&request)
        .expect("fixture subset may replay a declared nonhistorical bridge snapshot");

    assert_eq!(
        artifact.provenance.bridge_build_id,
        "fixture_subset_snapshot_20260901_a"
    );
    assert_eq!(artifact.summary.materialized_unique_accepted_loans, 2);
}

#[test]
fn synthetic_current_build_live_complete_materializes_relative_to_declared_snapshot() {
    let request = synthetic_current_build_live_complete_request();
    assert_ne!(
        request.provenance.bridge_build_id, CANON_GEO_H7_BRIDGE_BUILD_ID,
        "LiveComplete must not be pinned to the historical retained bridge build"
    );
    assert!(
        request
            .provenance
            .source_hashes
            .iter()
            .all(|hash| hash.source.starts_with("synthetic_current.")),
        "this fixture covers the live typed contract without claiming external proof"
    );

    let artifact = materialize_h7_population_rows(&request)
        .expect("synthetic current-build live shape materializes");

    assert_eq!(artifact.summary.source_rows, request.rows.len() as u64);
    assert_eq!(artifact.summary.materialized_unique_accepted_loans, 2);
    assert_eq!(artifact.summary.solver_population_subjects, 2);
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )
        .selected_multi_parcel_loans,
        1
    );
    assert_eq!(
        summary(
            &artifact.summary.truth_planes,
            GeoTruthPlane::RoundExactLenderParty
        )
        .selected_multi_parcel_loans,
        1
    );
    const {
        assert!(
            CANON_GEO_H7_FROZEN_E4_ACCEPTANCE_CASES == 79,
            "the frozen E4 gate remains a separate exact gate"
        );
    }
    assert!(
        artifact.summary.materialized_unique_accepted_loans
            < CANON_GEO_H7_FROZEN_E4_ACCEPTANCE_CASES,
        "LiveComplete typed-shape validation must not imply E4 gate success"
    );

    let evaluation =
        evaluate_population(&artifact.population).expect("synthetic solver population evaluates");
    assert_eq!(evaluation.summary.population_eligible_cases, 2);
    assert_eq!(evaluation.summary.candidate_reach_evaluated_cases, 2);
    assert_eq!(evaluation.summary.candidate_reach_partial_cases, 2);
    assert_eq!(
        evaluation.summary.solver_truth_scored_cases, 0,
        "solver truth stays unscored until candidate reach is full"
    );
}

#[test]
fn rejects_live_complete_denominator_inflation_over_materialized_subjects() {
    let mut request = synthetic_current_build_live_complete_request();
    request.plane_denominators[1].selected_multi_parcel_loans += 1;

    let error =
        materialize_h7_population_rows(&request).expect_err("inflated denominator rejected");
    assert!(
        error
            .message
            .contains("selected multi-parcel denominator must match materialized loan subjects")
    );
    assert_eq!(
        error.detail.get("truth_plane").map(String::as_str),
        Some("round_exact_lender_party")
    );
}

#[test]
fn rejects_empty_live_complete_population() {
    let mut request = synthetic_current_build_live_complete_request();
    request.rows.clear();
    request.provenance.observed_rows = 0;

    let error = materialize_h7_population_rows(&request)
        .expect_err("empty LiveComplete population rejected");
    assert!(
        error
            .message
            .contains("live population rows require nonzero fresh result rows"),
        "{}",
        error.message
    );
}

#[test]
fn rejects_live_complete_duplicate_loan_release_rows() {
    let mut request = synthetic_current_build_live_complete_request();
    request.rows.push(request.rows[0].clone());
    request.provenance.observed_rows = request.rows.len() as u64;

    let error =
        materialize_h7_population_rows(&request).expect_err("duplicate loan/release rejected");
    assert!(
        error
            .message
            .contains("repeat one loan/candidate-release measurement"),
        "{}",
        error.message
    );
}

#[test]
fn rejects_live_complete_empty_or_whitespace_bridge_build_identity() {
    for bridge_build_id in ["", " bridge_build_current_snapshot_20260901_a "] {
        let mut request = synthetic_current_build_live_complete_request();
        request.provenance.bridge_build_id = bridge_build_id.to_string();

        let error = materialize_h7_population_rows(&request)
            .expect_err("empty or whitespace build id rejected");
        assert!(
            error
                .message
                .contains("string fields must be non-empty and canonical-trimmed"),
            "{}",
            error.message
        );
        assert_eq!(
            error.detail.get("field").map(String::as_str),
            Some("provenance.bridge_build_id")
        );
    }
}

#[test]
fn rejects_live_complete_mixed_bridge_source_record_vintage() {
    let mut request = synthetic_current_build_live_complete_request();
    let stale_build = "1f4c62a1-5e78-409f-9f0d-111111111111";
    let bridge_record = request.rows[0]
        .source_records
        .iter_mut()
        .find(|record| record.role == GeoH7SourceRecordRole::BridgeLoan)
        .expect("bridge source record");
    bridge_record.source_record.source_vintage = stale_build.to_string();
    bridge_record.source_record.record_blake3 = blake3::hash(
        format!(
            "fixture-h7-source\0bridge_loan\0{}\0{stale_build}",
            bridge_record.source_record.source_record_id
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();

    let error = materialize_h7_population_rows(&request).expect_err("mixed build vintage rejected");
    assert!(
        error
            .message
            .contains("source evidence vintage does not match its required release")
    );
    assert_eq!(
        error.detail.get("role").map(String::as_str),
        Some("bridge_loan")
    );
    assert_eq!(
        error.detail.get("expected").map(String::as_str),
        Some(SYNTHETIC_CURRENT_BRIDGE_BUILD_ID)
    );
}

#[test]
fn h7_sql_uses_staging_columns_and_defers_borough_truth_to_legals() {
    for table in [
        "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER",
        "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES",
    ] {
        assert!(H7_STAGE2_SQL.contains(table), "Stage 2 must use {table}");
    }
    assert!(H7_STAGE2_SQL.contains("m.amount_cents = l.amount_cents"));
    assert!(H7_STAGE2_SQL.contains("m.recorded_date BETWEEN l.originationdate"));
    assert!(H7_STAGE2_SQL.contains("party.party_name_norm = l.originator_match_text"));
    assert!(H7_STAGE2_SQL.contains("m.document_row_rank = 1"));
    assert!(H7_STAGE2_SQL.contains("h7_stage2_master_party_candidate_row.v1"));
    assert!(H7_STAGE2_SQL.contains("acris_master_raw_csv_sha256"));
    assert!(H7_STAGE2_SQL.contains("acris_party_raw_csv_sha256s"));
    assert!(
        !H7_STAGE2_SQL.contains("ARRAY_CONTAINS(m.recorded_borough"),
        "MASTER recorded borough is diagnostic and must not reject candidates"
    );
    assert!(
        !H7_STAGE2_SQL.contains("NYC_ACRIS_REAL_PROPERTY_MASTER_EXT m"),
        "the repaired staging path must not fall back to raw MASTER"
    );

    assert!(H7_STAGE3_SQL.contains("h7_stage3_legal_residual_row.v1"));
    assert!(H7_STAGE3_SQL.contains("LATERAL FLATTEN(input => s.filed_boroughs)"));
    assert!(H7_STAGE3_SQL.contains("l.legal_borough = k.filed_borough"));
    assert!(H7_STAGE3_SQL.contains("STG_GEO_NYC_ACRIS_LEGALS"));
    assert!(
        !H7_STAGE3_SQL.contains("l.legal_borough = s.recorded_borough"),
        "LEGAL truth must agree with the filed borough, not MASTER metadata"
    );

    assert!(H7_DENOMINATOR_CONTROL_SQL.contains("accepted_multi_bbl_subjects"));
    assert!(H7_DENOMINATOR_CONTROL_SQL.contains("legal_document_count = 1"));
    assert!(H7_DENOMINATOR_CONTROL_SQL.contains("accepted_bbl_count > 1"));
    assert!(
        !H7_DENOMINATOR_CONTROL_SQL.contains("m.recorded_borough ="),
        "the full-plane control must preserve the same truth ordering"
    );

    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("h7_staging_accepted_truth_row.v0"));
    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("ARRAY_AGG(DISTINCT legal_bbl)"));
    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("ORDER BY legal_bbl"));
    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("acris_legal_source_records"));
    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("export_row_cap_reconciles"));
    assert!(H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("l.legal_borough = c.filed_borough"));
    assert!(
        !H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("l.legal_borough = c.recorded_borough"),
        "the accepted-truth export must bind LEGALS to filed borough"
    );
    assert!(
        !H7_ACCEPTED_TRUTH_EXPORT_SQL.contains("candidate_release"),
        "accepted legal truth must not pretend candidate reach has been measured"
    );

    assert!(H7_HALO_REACH_CONTROL_SQL.contains("STG_GEO_GEOMETRY_HOT_KEYS"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("H3_GRID_DISK"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("__BD2B9D_H7_HALO_K__"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("(SELECT halo_k FROM params)::TEXT"));
    assert!(!H7_HALO_REACH_CONTROL_SQL.contains("point_k1'::TEXT"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("section_candidate_edges"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("candidate_bbl IS NOT NULL"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("full_reach_subjects"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("partial_reach_subjects"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("no_reach_subjects"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("empty_work_sections"));
    assert!(H7_HALO_REACH_CONTROL_SQL.contains("accepted_truth_repeats_loan"));
    assert!(
        !H7_HALO_REACH_CONTROL_SQL.contains("STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES"),
        "the bounded reach control must use the populated H3 key index"
    );
    assert!(
        !H7_HALO_REACH_CONTROL_SQL.contains("H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(p.centroid"),
        "the bounded reach control must not recompute all parcel H3 keys"
    );

    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("pip_edges"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("pip_blocks"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("SUBSTR(p.bbl_key, 1, 6)"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("ST_CONTAINS("));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("full_reach_subjects"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("partial_reach_subjects"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("no_reach_subjects"));
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("reach_accounting_failures"));
    assert!(
        H7_PIP_BLOCK_REACH_CONTROL_SQL
            .contains("COUNT(DISTINCT IFF(c.candidate_bbl IS NOT NULL, t.truth_bbl, NULL))")
    );
    assert!(H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("accepted_truth_repeats_loan"));
    assert!(
        !H7_PIP_BLOCK_REACH_CONTROL_SQL.contains("propertyaddress"),
        "candidate blocks must not be seeded through the address channel"
    );
    let truth_edges = H7_PIP_BLOCK_REACH_CONTROL_SQL
        .find("truth_edges AS")
        .expect("truth comparison CTE");
    for candidate_cte in ["pip_edges AS", "pip_blocks AS", "candidate_edges AS"] {
        assert!(
            H7_PIP_BLOCK_REACH_CONTROL_SQL
                .find(candidate_cte)
                .is_some_and(|offset| offset < truth_edges),
            "{candidate_cte} must be constructed before truth is flattened"
        );
    }

    assert!(H7_INCIDENCE_SHARD_SQL.contains("STG_GEO_GEOMETRY_HOT_KEYS"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("H3_GRID_DISK"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("parcel_components"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("truth_outside_k1"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("k1_multi"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("nonzero_work_unit_sanity"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("component_shape_sanity"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("component_accounting_sanity"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("NYC_BUILDING_FOOTPRINTS_HOT"));
    assert!(H7_INCIDENCE_SHARD_SQL.contains("OVERTURE_MAPS_FEATURES_HOT"));
    assert!(
        H7_INCIDENCE_SHARD_SQL
            .contains("REGEXP_REPLACE(TO_CHAR(p.bbl), '[.]0$', '') = k.parcel_id"),
        "raw geom-v3 BBLs must be normalized before joining staging keys"
    );
    assert!(
        !H7_INCIDENCE_SHARD_SQL.contains("p.bbl = k.parcel_id"),
        "numeric raw BBL rendering must not be compared directly with the text key"
    );
}

#[test]
fn h7_schemas_cover_real_row_and_artifact_instances() {
    let request = base_request();
    let artifact = materialize_h7_population_rows(&request).expect("h7 replay materializes");
    let rows_schema: Value =
        serde_json::from_str(H7_POPULATION_ROWS_SCHEMA).expect("rows schema is JSON");
    let artifact_schema: Value =
        serde_json::from_str(H7_POPULATION_SCHEMA).expect("artifact schema is JSON");

    assert_schema_contract(
        &rows_schema,
        "canon.geo.h7_population_rows.v0",
        CANON_GEO_H7_POPULATION_ROWS_VERSION,
    );
    assert_schema_contract(
        &artifact_schema,
        "canon.geo.h7_population.v0",
        CANON_GEO_H7_POPULATION_VERSION,
    );
    assert_eq!(
        rows_schema
            .pointer("/$defs/provenance/properties/collateral_scope/const")
            .and_then(Value::as_str),
        Some(CANON_GEO_H7_COLLATERAL_SCOPE)
    );
    assert_eq!(
        rows_schema
            .pointer("/$defs/warehouse_row/properties/property_state/const")
            .and_then(Value::as_str),
        Some("NY")
    );
    assert_eq!(
        rows_schema
            .pointer("/$defs/warehouse_row/properties/truth_parcels/minItems")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert!(
        rows_schema
            .pointer("/$defs/warehouse_row/properties/candidate_parcels/minItems")
            .is_none(),
        "warehouse rows must represent an honest zero-candidate reach result"
    );
    assert!(
        rows_schema
            .pointer("/$defs/query_receipt/properties/truth_plane")
            .is_some(),
        "query receipt schema must expose truth_plane for legal residuals"
    );

    let request_value = serde_json::to_value(&request).expect("serialize rows request");
    assert_schema_declares_object_keys(&rows_schema, "", &request_value);
    assert_schema_declares_object_keys(
        &rows_schema,
        "/$defs/provenance",
        &request_value["provenance"],
    );
    assert_schema_declares_object_keys(
        &rows_schema,
        "/$defs/plane_denominator",
        &request_value["plane_denominators"][0],
    );
    assert_schema_declares_object_keys(
        &rows_schema,
        "/$defs/warehouse_row",
        &request_value["rows"][0],
    );
    assert_schema_declares_object_keys(
        &rows_schema,
        "/$defs/source_evidence_record",
        &request_value["rows"][0]["source_records"][0],
    );

    let artifact_value = serde_json::to_value(&artifact).expect("serialize h7 artifact");
    assert_schema_declares_object_keys(&artifact_schema, "", &artifact_value);
    assert_schema_declares_object_keys(
        &artifact_schema,
        "/$defs/summary",
        &artifact_value["summary"],
    );
    assert_schema_declares_object_keys(
        &artifact_schema,
        "/$defs/truth_plane_summary",
        &artifact_value["summary"]["truth_planes"][0],
    );
    assert_schema_declares_object_keys(
        &artifact_schema,
        "/$defs/case",
        &artifact_value["cases"][0],
    );
    assert_eq!(
        artifact_schema
            .pointer("/$defs/case/properties/property_state/const")
            .and_then(Value::as_str),
        Some("NY")
    );
    assert_eq!(
        artifact_schema
            .pointer("/$defs/case/properties/truth_parcels/minItems")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert!(
        artifact_schema
            .pointer("/$defs/case/properties/candidate_parcels/minItems")
            .is_none(),
        "case artifacts must preserve zero-candidate upstream failures"
    );
    assert_schema_declares_object_keys(
        &artifact_schema,
        "/$defs/source_evidence_record",
        &artifact_value["cases"][0]["source_records"][0],
    );
}

#[test]
fn rejects_h7_rows_json_missing_required_provenance_vectors() {
    let mut value = serde_json::to_value(base_request()).expect("serialize fixture");
    value
        .pointer_mut("/provenance")
        .expect("provenance object")
        .as_object_mut()
        .expect("provenance object")
        .remove("source_hashes");

    let error =
        serde_json::from_value::<GeoH7PopulationRowsRequest>(value).expect_err("missing field");
    assert!(error.to_string().contains("source_hashes"));
}

#[test]
fn rejects_non_h7_plane_pooling() {
    let mut request = base_request();
    request.rows[0].truth_plane = GeoTruthPlane::GateV2Historical;

    let error = materialize_h7_population_rows(&request).expect_err("pooled plane rejected");
    assert!(error.message.contains("controlling H.7 truth planes"));
}

#[test]
fn rejects_duplicate_inflation_across_accepted_docs_or_planes() {
    let mut request = base_request();
    request.rows[1].document_id = "doc-conflict".to_string();

    let error = materialize_h7_population_rows(&request).expect_err("conflicting accepted truth");
    assert!(error.message.contains("conflicting accepted truth"));
}

#[test]
fn rejects_empty_truth_sets() {
    let mut request = base_request();
    request.rows[0].truth_parcels.clear();

    let error = materialize_h7_population_rows(&request).expect_err("empty truth rejected");
    assert!(error.message.contains("non-empty parcel sets"));
}

#[test]
fn retains_zero_candidate_reach_without_manufacturing_a_solver_case() {
    let mut request = base_request();
    for row in request
        .rows
        .iter_mut()
        .filter(|row| row.loan_key == "loan-nonround")
    {
        row.candidate_parcels.clear();
        row.reach_status = GeoH7CandidateReachStatus::None;
        row.reach_reason = "address_blind_pip_returned_no_candidate".to_string();
        row.source_records
            .retain(|record| record.role != GeoH7SourceRecordRole::MapplutoCandidate);
    }

    let artifact = materialize_h7_population_rows(&request).expect("zero reach is materialized");
    let non_round = summary(
        &artifact.summary.truth_planes,
        GeoTruthPlane::NonRoundAmountDateLegalBorough,
    );

    assert_eq!(artifact.cases.len(), 4);
    assert_eq!(non_round.candidate_reach_none_cases, 2);
    assert_eq!(non_round.candidate_parcels, 0);
    assert_eq!(artifact.summary.solver_population_subjects, 1);
    assert_eq!(artifact.population.cases.len(), 1);
    assert!(
        artifact
            .population
            .cases
            .iter()
            .all(|case| case.id != artifact.cases[0].subject_id)
    );
}

#[test]
fn rejects_mappluto_provenance_when_candidate_set_is_empty() {
    let mut request = base_request();
    request.rows[0].candidate_parcels.clear();
    request.rows[0].reach_status = GeoH7CandidateReachStatus::None;

    let error = materialize_h7_population_rows(&request)
        .expect_err("stale candidate provenance must not survive zero reach");
    assert!(error.message.contains("parcel union must equal"));
    assert_eq!(
        error.detail.get("role").map(String::as_str),
        Some("mappluto_candidate")
    );
    assert_eq!(
        error.detail.get("mismatch").map(String::as_str),
        Some("extra")
    );
}

#[test]
fn rejects_mappluto_release_drift() {
    let mut request = base_request();
    request.rows[0].candidate_release.release = "26v3".to_string();

    let error = materialize_h7_population_rows(&request).expect_err("release drift rejected");
    assert!(error.message.contains("pinned MapPLUTO"));
}

#[test]
fn rejects_candidate_reach_conflation() {
    let mut request = base_request();
    request.rows[1].reach_status = GeoH7CandidateReachStatus::Full;

    let error = materialize_h7_population_rows(&request).expect_err("reach mismatch rejected");
    assert!(error.message.contains("candidate reach"));
}

#[test]
fn rejects_denominator_algebra_drift() {
    let mut request = base_request();
    request.plane_denominators[0].candidate_loans = 221;

    let error = materialize_h7_population_rows(&request).expect_err("bad algebra rejected");
    assert!(error.message.contains("no-legal-confirmation"));
}

#[test]
fn admits_multi_filed_borough_truth_when_edges_reconcile() {
    let mut request = base_request();
    request.rows[0].loan_field_distinct_counts.filed_borough = 2;
    request.rows[0].accepted_borough_edges =
        vec![borough_edge("KINGS", 3), borough_edge("RICHMOND", 5)];
    request.rows[1].loan_field_distinct_counts.filed_borough = 2;
    request.rows[1].accepted_borough_edges =
        vec![borough_edge("KINGS", 3), borough_edge("RICHMOND", 5)];

    let artifact = materialize_h7_population_rows(&request).expect("multi-borough row admitted");
    let non_round_cases = artifact
        .cases
        .iter()
        .filter(|case| case.truth_plane == GeoTruthPlane::NonRoundAmountDateLegalBorough)
        .collect::<Vec<_>>();
    assert_eq!(non_round_cases.len(), 2);
    assert!(
        non_round_cases
            .iter()
            .all(|case| case.accepted_borough_edges.len() == 2)
    );
}

#[test]
fn rejects_cherry_picked_candidate_release_rows() {
    let mut request = base_request();
    request
        .rows
        .retain(|row| !(row.loan_key == "loan-round" && row.candidate_release.release == "26v2"));

    let error = materialize_h7_population_rows(&request).expect_err("missing release rejected");
    assert!(error.message.contains("every pinned candidate release"));
}

#[test]
fn rejects_retained_complete_without_retained_receipts_even_when_count_shaped() {
    let mut request = synthetic_complete_shape_request();
    request.provenance.observed_rows = request.rows.len() as u64;
    request.provenance.source_hashes.clear();
    request.provenance.query_receipts = vec![receipt("fixture_count_shaped_only", 1)];

    let error =
        materialize_h7_population_rows(&request).expect_err("retained receipts are required");
    assert!(
        error
            .message
            .contains("retained-complete population requires preserved source hashes")
    );
}

#[test]
fn rejects_retained_complete_without_payload_observed_row_binding() {
    let mut request = synthetic_complete_shape_request();
    request.provenance.source_hashes = vec![GeoH7SourceHash {
        source: "fixture_retained_complete_sources".to_string(),
        hash_kind: "fixture_sha256".to_string(),
        sha256: "0".repeat(64),
    }];
    request.provenance.query_receipts = vec![real_query_receipt(
        "retained_h7_candidate_legal_residual",
        "synthetic-retained-candidate-legal",
        "SELECT loan_key, document_id, legal_borough, bbl FROM synthetic_retained_h7_candidate_legal",
        GeoH7QueryDisposition::Cited,
        1,
    )];
    request.provenance.observed_rows = 0;

    let error = materialize_h7_population_rows(&request).expect_err("retained rows mismatch");
    assert!(error.message.contains("actual input payload"));
}

#[test]
fn rejects_fixture_subset_claiming_retained_complete_counts() {
    let mut request = base_request();
    request.plane_denominators[0].selected_multi_parcel_loans = 35;
    request.plane_denominators[1].selected_multi_parcel_loans = 14;

    let error = materialize_h7_population_rows(&request).expect_err("subset count claim rejected");
    assert!(
        error
            .message
            .contains("selected multi-parcel denominator must match")
    );
}

#[test]
fn rejects_richmond_ga_from_raw_filed_state_guard() {
    let mut request = base_request();
    request.rows[0].filed_county = "RICHMOND".to_string();
    request.rows[0].filed_borough = 5;
    request.rows[0].legal_borough = 5;
    request.rows[0].accepted_borough_edges = vec![borough_edge("RICHMOND", 5)];
    request.rows[0].property_state = "GA".to_string();

    let error = materialize_h7_population_rows(&request).expect_err("GA state rejected");
    assert!(error.message.contains("property_state NY"));
}

#[test]
fn rejects_non_h7_collateral_scope() {
    let mut request = base_request();
    request.provenance.collateral_scope = "full_national_collateral".to_string();

    let error = materialize_h7_population_rows(&request).expect_err("scope rejected");
    assert!(error.message.contains("NYC filed-collateral slice"));
}

#[test]
fn rejects_ambiguous_bridge_values_before_plane_classification() {
    let mut request = base_request();
    request.rows[0]
        .loan_field_distinct_counts
        .originalloanamount = 2;

    let error = materialize_h7_population_rows(&request).expect_err("ambiguous amount rejected");
    assert!(error.message.contains("loan amounts at loan grain"));
}

#[test]
fn accepts_multiple_distinct_source_records_per_required_role() {
    let mut request = base_request();
    request.rows[0].source_records.push(source_record(
        GeoH7SourceRecordRole::BridgeLoan,
        "loan-nonround:bridge-property-2",
        CANON_GEO_H7_BRIDGE_BUILD_ID,
        &[],
    ));

    let artifact = materialize_h7_population_rows(&request).expect("multi-record roles admitted");
    let non_round_26v1 = artifact
        .cases
        .iter()
        .find(|case| case.loan_key == "loan-nonround" && case.candidate_release.release == "26v1")
        .expect("non-round 26v1 case");
    assert!(
        non_round_26v1
            .source_records
            .iter()
            .filter(|record| record.role == GeoH7SourceRecordRole::AcrisLegal)
            .count()
            > 1
    );
}

#[test]
fn rejects_missing_required_h7_source_role() {
    let mut request = base_request();
    request.rows[0]
        .source_records
        .retain(|record| record.role != GeoH7SourceRecordRole::AcrisLegal);

    let error = materialize_h7_population_rows(&request).expect_err("missing source role rejected");
    assert!(
        error
            .message
            .contains("missing a required source evidence role")
    );
    assert_eq!(
        error.detail.get("required_role").map(String::as_str),
        Some("acris_legal")
    );
}

#[test]
fn rejects_diagnostic_source_as_required_h7_role() {
    let mut request = base_request();
    for record in &mut request.rows[0].source_records {
        if record.role == GeoH7SourceRecordRole::MapplutoCandidate {
            record.role = GeoH7SourceRecordRole::GeocodeDiagnostic;
            record.parcel_ids.clear();
        }
    }

    let error =
        materialize_h7_population_rows(&request).expect_err("diagnostic role cannot satisfy truth");
    assert!(
        error
            .message
            .contains("missing a required source evidence role")
    );
    assert_eq!(
        error.detail.get("required_role").map(String::as_str),
        Some("mappluto_candidate")
    );
}

#[test]
fn rejects_duplicate_h7_source_record_id_across_roles() {
    let mut request = base_request();
    let duplicate_id = request.rows[0].source_records[0]
        .source_record
        .source_record_id
        .clone();
    request.rows[0].source_records[1]
        .source_record
        .source_record_id = duplicate_id;

    let error = materialize_h7_population_rows(&request).expect_err("duplicate source id rejected");
    assert!(
        error
            .message
            .contains("repeat an immutable source record id")
    );
}

#[test]
fn rejects_acris_legal_source_that_does_not_cover_every_truth_parcel() {
    let mut request = base_request();
    request.rows[0].source_records.retain(|record| {
        record.role != GeoH7SourceRecordRole::AcrisLegal
            || record.parcel_ids != vec!["1000000002".to_string()]
    });

    let error =
        materialize_h7_population_rows(&request).expect_err("truth parcel support rejected");
    assert!(error.message.contains("parcel union must equal"));
    assert_eq!(
        error.detail.get("role").map(String::as_str),
        Some("acris_legal")
    );
    assert_eq!(
        error.detail.get("mismatch").map(String::as_str),
        Some("missing")
    );
}

#[test]
fn rejects_mappluto_source_that_does_not_cover_every_candidate_parcel() {
    let mut request = base_request();
    request.rows[0].source_records.retain(|record| {
        record.role != GeoH7SourceRecordRole::MapplutoCandidate
            || record.parcel_ids != vec!["1000000003".to_string()]
    });

    let error =
        materialize_h7_population_rows(&request).expect_err("candidate parcel support rejected");
    assert!(error.message.contains("parcel union must equal"));
    assert_eq!(
        error.detail.get("role").map(String::as_str),
        Some("mappluto_candidate")
    );
    assert_eq!(
        error.detail.get("mismatch").map(String::as_str),
        Some("missing")
    );
}

#[test]
fn rejects_acris_legal_source_that_claims_two_truth_parcels() {
    let mut request = base_request();
    request.rows[0]
        .source_records
        .retain(|record| record.role != GeoH7SourceRecordRole::AcrisLegal);
    request.rows[0].source_records.push(source_record(
        GeoH7SourceRecordRole::AcrisLegal,
        "doc-nonround:legal-bbl-spanning-two",
        CANON_GEO_H7_ACRIS_RELEASE_DT,
        &["1000000001", "1000000002"],
    ));

    let error = materialize_h7_population_rows(&request).expect_err("two-parcel legal rejected");
    assert!(error.message.contains("must name exactly one parcel"));
}

#[test]
fn rejects_mappluto_source_that_claims_two_candidate_parcels() {
    let mut request = base_request();
    request.rows[0]
        .source_records
        .retain(|record| record.role != GeoH7SourceRecordRole::MapplutoCandidate);
    request.rows[0].source_records.push(source_record(
        GeoH7SourceRecordRole::MapplutoCandidate,
        "loan-nonround:26v1:mappluto-spanning-two",
        "2026-05-01",
        &["1000000001", "1000000002"],
    ));

    let error =
        materialize_h7_population_rows(&request).expect_err("two-parcel candidate rejected");
    assert!(error.message.contains("must name exactly one parcel"));
}

#[test]
fn rejects_extra_acris_legal_source_parcel() {
    let mut request = base_request();
    request.rows[0].source_records.push(source_record(
        GeoH7SourceRecordRole::AcrisLegal,
        "doc-nonround:legal-extra",
        CANON_GEO_H7_ACRIS_RELEASE_DT,
        &["1000000999"],
    ));

    let error = materialize_h7_population_rows(&request).expect_err("extra legal rejected");
    assert!(error.message.contains("parcel union must equal"));
    assert_eq!(
        error.detail.get("mismatch").map(String::as_str),
        Some("extra")
    );
}

#[test]
fn rejects_extra_mappluto_source_parcel() {
    let mut request = base_request();
    request.rows[1].source_records.push(source_record(
        GeoH7SourceRecordRole::MapplutoCandidate,
        "loan-nonround:26v2:mappluto-extra",
        "2026-08-01",
        &["1000000999"],
    ));

    let error = materialize_h7_population_rows(&request).expect_err("extra mappluto rejected");
    assert!(error.message.contains("parcel union must equal"));
    assert_eq!(
        error.detail.get("mismatch").map(String::as_str),
        Some("extra")
    );
}

#[test]
fn rejects_h7_source_vintage_release_mismatch() {
    let mut request = base_request();
    request.rows[1]
        .source_records
        .iter_mut()
        .find(|record| record.role == GeoH7SourceRecordRole::MapplutoCandidate)
        .expect("mappluto role")
        .source_record
        .source_vintage = "2026-05-01".to_string();

    let error =
        materialize_h7_population_rows(&request).expect_err("source vintage mismatch rejected");
    assert!(error.message.contains("source evidence vintage"));
}

#[test]
fn rejects_empirical_discrepancy_without_registered_receipt() {
    let mut request = base_request();
    request.provenance.empirical_discrepancies[0].receipt_ids =
        vec!["unregistered-receipt".to_string()];

    let error =
        materialize_h7_population_rows(&request).expect_err("unregistered receipt rejected");
    assert!(error.message.contains("unregistered receipt id"));
}

#[test]
fn rejects_real_query_receipt_hashed_from_purpose_only() {
    let mut request = base_request();
    request.provenance.query_receipts = vec![real_query_receipt(
        "synthetic_live_raw_property_state_ny_control",
        "synthetic-query-state-control",
        "SELECT truth_plane, eligible_loans FROM synthetic_h7_state_control",
        GeoH7QueryDisposition::Cited,
        7,
    )];
    request.provenance.query_receipts[0].query_blake3 =
        blake3::hash(request.provenance.query_receipts[0].purpose.as_bytes())
            .to_hex()
            .to_string();
    request.provenance.query_receipts[0].query_text_ref = format!(
        "warehouse_query_history/{}@blake3:{}",
        request.provenance.query_receipts[0]
            .query_id
            .as_deref()
            .expect("query id"),
        request.provenance.query_receipts[0].query_blake3
    );
    request.provenance.query_receipts[0].normalized_query_text = None;

    let error = materialize_h7_population_rows(&request).expect_err("purpose hash rejected");
    assert!(error.message.contains("must bind SQL text"));
}

#[test]
fn rejects_release_row_representative_borough_drift() {
    let mut request = base_request();
    request.rows[0].loan_field_distinct_counts.filed_borough = 2;
    request.rows[0].accepted_borough_edges =
        vec![borough_edge("KINGS", 3), borough_edge("RICHMOND", 5)];
    request.rows[1].loan_field_distinct_counts.filed_borough = 2;
    request.rows[1].filed_county = "RICHMOND".to_string();
    request.rows[1].filed_borough = 5;
    request.rows[1].legal_borough = 5;
    request.rows[1].accepted_borough_edges =
        vec![borough_edge("KINGS", 3), borough_edge("RICHMOND", 5)];

    let error =
        materialize_h7_population_rows(&request).expect_err("representative drift rejected");
    assert!(
        error
            .message
            .contains("canonical accepted-borough representative")
    );
}

#[test]
fn rejects_live_payload_without_matching_fresh_rows() {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::LiveComplete;
    request.provenance.result_mode = GeoH7ResultMode::Live;
    request.provenance.observed_rows = 2;
    request.provenance.source_hashes = vec![GeoH7SourceHash {
        source: "source.nyc_acris_real_property_master_ext".to_string(),
        hash_kind: "raw_csv_sha256".to_string(),
        sha256: "0".repeat(64),
    }];
    install_live_query_receipts(&mut request);
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::NonRoundAmountDateLegalBorough,
            1,
        ));
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::RoundExactLenderParty,
            1,
        ));

    let error = materialize_h7_population_rows(&request).expect_err("live mismatch rejected");
    assert!(error.message.contains("actual input payload"));
}

#[test]
fn rejects_live_complete_without_non_round_candidate_legal_residual_receipt() {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::LiveComplete;
    request.provenance.result_mode = GeoH7ResultMode::Live;
    request.provenance.observed_rows = request.rows.len() as u64;
    request.provenance.source_hashes = vec![GeoH7SourceHash {
        source: "source.nyc_acris_real_property_master_ext".to_string(),
        hash_kind: "raw_csv_sha256".to_string(),
        sha256: "0".repeat(64),
    }];
    install_live_query_receipts(&mut request);
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::RoundExactLenderParty,
            1,
        ));

    let error =
        materialize_h7_population_rows(&request).expect_err("missing non-round residual rejected");
    assert!(error.message.contains("candidate/legal residual"));
    assert_eq!(
        error.detail.get("truth_plane").map(String::as_str),
        Some("non_round_amount_date_legal_borough")
    );
    assert_eq!(
        error
            .detail
            .get("required_receipt_purpose")
            .map(String::as_str),
        Some(CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE)
    );
}

#[test]
fn rejects_live_complete_without_round_candidate_legal_residual_receipt() {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::LiveComplete;
    request.provenance.result_mode = GeoH7ResultMode::Live;
    request.provenance.observed_rows = request.rows.len() as u64;
    request.provenance.source_hashes = vec![GeoH7SourceHash {
        source: "source.nyc_acris_real_property_master_ext".to_string(),
        hash_kind: "raw_csv_sha256".to_string(),
        sha256: "0".repeat(64),
    }];
    install_live_query_receipts(&mut request);
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::NonRoundAmountDateLegalBorough,
            1,
        ));

    let error =
        materialize_h7_population_rows(&request).expect_err("missing round residual rejected");
    assert!(error.message.contains("candidate/legal residual"));
    assert_eq!(
        error.detail.get("truth_plane").map(String::as_str),
        Some("round_exact_lender_party")
    );
    assert_eq!(
        error
            .detail
            .get("required_receipt_purpose")
            .map(String::as_str),
        Some(CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE)
    );
}

#[test]
fn rejects_live_round_plane_with_zero_row_candidate_legal_residual_receipt() {
    let mut request = base_request();
    request.population_scope = GeoH7PopulationScope::LiveComplete;
    request.provenance.result_mode = GeoH7ResultMode::Live;
    request.provenance.observed_rows = request.rows.len() as u64;
    request.provenance.source_hashes = vec![GeoH7SourceHash {
        source: "source.nyc_acris_real_property_master_ext".to_string(),
        hash_kind: "raw_csv_sha256".to_string(),
        sha256: "0".repeat(64),
    }];
    install_live_query_receipts(&mut request);
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::NonRoundAmountDateLegalBorough,
            1,
        ));
    request
        .provenance
        .query_receipts
        .push(legal_residual_receipt(
            GeoTruthPlane::RoundExactLenderParty,
            0,
        ));

    let error = materialize_h7_population_rows(&request).expect_err("zero-row residual rejected");
    assert!(error.message.contains("nonzero result rows"));
}

#[test]
fn rejects_legal_residual_receipt_with_wrong_truth_plane() {
    let mut request = base_request();
    request.provenance.query_receipts = vec![legal_residual_receipt(
        GeoTruthPlane::NonRoundAmountDateLegalBorough,
        1,
    )];
    request.provenance.query_receipts[0].truth_plane = Some(GeoTruthPlane::RoundExactLenderParty);

    let error = materialize_h7_population_rows(&request).expect_err("receipt truth plane rejected");
    assert!(error.message.contains("purpose must match its truth_plane"));
}

fn summary(
    summaries: &[GeoH7TruthPlaneSummary],
    truth_plane: GeoTruthPlane,
) -> &GeoH7TruthPlaneSummary {
    summaries
        .iter()
        .find(|summary| summary.truth_plane == truth_plane)
        .expect("truth-plane summary")
}

fn assert_schema_contract(schema: &Value, title: &str, version: &str) {
    assert_eq!(schema.get("title").and_then(Value::as_str), Some(title));
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(version)
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
}

fn assert_schema_declares_object_keys(schema: &Value, pointer: &str, instance: &Value) {
    let schema_object = if pointer.is_empty() {
        schema
    } else {
        schema.pointer(pointer).expect("schema pointer resolves")
    };
    let declared = schema_object
        .get("properties")
        .and_then(Value::as_object)
        .expect("object schema has properties");
    let actual = instance.as_object().expect("instance is object");
    for key in actual.keys() {
        assert!(
            declared.contains_key(key),
            "{pointer}: schema does not declare serialized key {key}"
        );
    }
}

fn non_round_row(loan_key: &str, document_id: &str, release: &str) -> GeoH7PopulationWarehouseRow {
    h7_row(H7RowFixture {
        loan_key,
        document_id,
        truth_plane: GeoTruthPlane::NonRoundAmountDateLegalBorough,
        release,
        amount_cents: 12_345_678,
        is_round_100k_lattice: false,
        lender_match_text: None,
        lender_party_type: None,
        doc_type: "MTGE",
        geocoded_county_fips: Some("36061"),
    })
}

fn round_row(loan_key: &str, document_id: &str, release: &str) -> GeoH7PopulationWarehouseRow {
    h7_row(H7RowFixture {
        loan_key,
        document_id,
        truth_plane: GeoTruthPlane::RoundExactLenderParty,
        release,
        amount_cents: 50_000_000,
        is_round_100k_lattice: true,
        lender_match_text: Some("ACME BANK"),
        lender_party_type: Some("1"),
        doc_type: "MMTG",
        geocoded_county_fips: Some("36047"),
    })
}

struct H7RowFixture<'a> {
    loan_key: &'a str,
    document_id: &'a str,
    truth_plane: GeoTruthPlane,
    release: &'a str,
    amount_cents: u64,
    is_round_100k_lattice: bool,
    lender_match_text: Option<&'a str>,
    lender_party_type: Option<&'a str>,
    doc_type: &'a str,
    geocoded_county_fips: Option<&'a str>,
}

fn h7_row(fixture: H7RowFixture<'_>) -> GeoH7PopulationWarehouseRow {
    let H7RowFixture {
        loan_key,
        document_id,
        truth_plane,
        release,
        amount_cents,
        is_round_100k_lattice,
        lender_match_text,
        lender_party_type,
        doc_type,
        geocoded_county_fips,
    } = fixture;
    let originator_count = if lender_match_text.is_some() { 1 } else { 0 };
    let candidate_release = mappluto_pin(release);
    let mut source_records = vec![
        source_record(
            GeoH7SourceRecordRole::BridgeLoan,
            &format!("{loan_key}:bridge-loan"),
            CANON_GEO_H7_BRIDGE_BUILD_ID,
            &[],
        ),
        source_record(
            GeoH7SourceRecordRole::AcrisMaster,
            &format!("{document_id}:master"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[],
        ),
        source_record(
            GeoH7SourceRecordRole::AcrisLegal,
            &format!("{document_id}:legal-bbl-1"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &["1000000001"],
        ),
        source_record(
            GeoH7SourceRecordRole::AcrisLegal,
            &format!("{document_id}:legal-bbl-2"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &["1000000002"],
        ),
        source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-1"),
            candidate_release.release_dt.as_str(),
            &["1000000001"],
        ),
    ];
    if release == "26v1" {
        source_records.push(source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-2"),
            candidate_release.release_dt.as_str(),
            &["1000000002"],
        ));
        source_records.push(source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-3"),
            candidate_release.release_dt.as_str(),
            &["1000000003"],
        ));
    }
    if lender_match_text.is_some() {
        source_records.push(source_record(
            GeoH7SourceRecordRole::AcrisParty,
            &format!("{document_id}:party:{release}"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[],
        ));
    }
    GeoH7PopulationWarehouseRow {
        loan_key: loan_key.to_string(),
        document_id: document_id.to_string(),
        truth_plane,
        association_plane: GeoH7AssociationPlane::MultiProperty,
        candidate_release,
        property_state: "NY".to_string(),
        filed_county: "KINGS".to_string(),
        filed_borough: 3,
        legal_borough: 3,
        accepted_borough_edges: vec![borough_edge("KINGS", 3)],
        geocoded_county_fips: geocoded_county_fips.map(str::to_string),
        doc_type: doc_type.to_string(),
        originationdate: "2025-01-15".to_string(),
        amount_cents,
        is_round_100k_lattice,
        originatorname: lender_match_text.map(|_| "Acme Bank".to_string()),
        originator_match_text: lender_match_text.map(str::to_string),
        lender_match_text: lender_match_text.map(str::to_string),
        lender_party_type: lender_party_type.map(str::to_string),
        loan_field_distinct_counts: canon::geo::GeoH7LoanFieldDistinctCounts {
            originatorname: originator_count,
            originator_match_text: originator_count,
            originationdate: 1,
            originalloanamount: 1,
            filed_borough: 1,
        },
        truth_parcels: vec!["1000000002".to_string(), "1000000001".to_string()],
        candidate_parcels: if release == "26v2" {
            vec!["1000000001".to_string()]
        } else {
            vec![
                "1000000003".to_string(),
                "1000000002".to_string(),
                "1000000001".to_string(),
            ]
        },
        reach_status: if release == "26v2" {
            GeoH7CandidateReachStatus::Partial
        } else {
            GeoH7CandidateReachStatus::Full
        },
        reach_reason: "candidate_release_scored_against_same_accepted_truth".to_string(),
        source_records,
    }
}

fn mappluto_pin(release: &str) -> GeoH7MapplutoReleasePin {
    let release_dt = match release {
        "26v1" => "2026-05-01",
        "26v2" => "2026-08-01",
        _ => "2026-12-01",
    };
    GeoH7MapplutoReleasePin {
        release: release.to_string(),
        release_dt: release_dt.to_string(),
        variant: "shoreline_clipped".to_string(),
        geometry_contract_version: CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION.to_string(),
    }
}

fn borough_edge(filed_county: &str, borough: u8) -> GeoH7BoroughEdge {
    GeoH7BoroughEdge {
        filed_county: filed_county.to_string(),
        filed_borough: borough,
        legal_borough: borough,
    }
}

fn filed_county_mapping() -> Vec<GeoH7FiledCountyMapping> {
    [
        ("NEW YORK", 1),
        ("MANHATTAN", 1),
        ("NY061", 1),
        ("BRONX", 2),
        ("KINGS", 3),
        ("BROOKLYN", 3),
        ("QUEENS", 4),
        ("RICHMOND", 5),
    ]
    .into_iter()
    .map(|(filed_county, acris_borough)| GeoH7FiledCountyMapping {
        filed_county: filed_county.to_string(),
        acris_borough,
    })
    .collect()
}

fn source_record(
    role: GeoH7SourceRecordRole,
    source_record_id: &str,
    source_vintage: &str,
    parcel_ids: &[&str],
) -> GeoH7SourceEvidenceRecord {
    GeoH7SourceEvidenceRecord {
        role,
        parcel_ids: parcel_ids
            .iter()
            .map(|parcel_id| (*parcel_id).to_string())
            .collect(),
        source_record: GeoEvidenceRecordRef {
            source_record_id: source_record_id.to_string(),
            source_vintage: source_vintage.to_string(),
            record_blake3: blake3::hash(
                format!(
                    "fixture-h7-source\0{}\0{source_record_id}\0{source_vintage}",
                    role_name(role)
                )
                .as_bytes(),
            )
            .to_hex()
            .to_string(),
        },
    }
}

fn retag_bridge_source_records(request: &mut GeoH7PopulationRowsRequest, bridge_build_id: &str) {
    for row in &mut request.rows {
        for record in &mut row.source_records {
            if record.role == GeoH7SourceRecordRole::BridgeLoan {
                record.source_record.source_vintage = bridge_build_id.to_string();
                record.source_record.record_blake3 = blake3::hash(
                    format!(
                        "fixture-h7-source\0bridge_loan\0{}\0{bridge_build_id}",
                        record.source_record.source_record_id
                    )
                    .as_bytes(),
                )
                .to_hex()
                .to_string();
            }
        }
    }
}

fn external_receipt(
    receipt_id: &str,
    kind: GeoH7ExternalReceiptKind,
    purpose: &str,
) -> GeoH7ExternalReceiptRef {
    GeoH7ExternalReceiptRef {
        receipt_id: receipt_id.to_string(),
        kind,
        purpose: purpose.to_string(),
    }
}

fn receipt(purpose: &str, result_rows: u64) -> GeoH7QueryReceipt {
    let query_text_ref = format!("fixture:h7-replay:{purpose}");
    let fixture_text = format!("{query_text_ref}:synthetic-diagnostic");
    GeoH7QueryReceipt {
        purpose: purpose.to_string(),
        truth_plane: None,
        query_id: None,
        query_text_ref,
        normalized_query_text: None,
        query_blake3: blake3::hash(fixture_text.as_bytes()).to_hex().to_string(),
        result_rows,
        row_cap: 100,
        disposition: GeoH7QueryDisposition::DiagnosticOnly,
    }
}

fn cancelled_receipt(purpose: &str) -> GeoH7QueryReceipt {
    let query_text_ref = format!("fixture:h7-replay:{purpose}");
    let fixture_text = format!("{query_text_ref}:synthetic-cancelled");
    GeoH7QueryReceipt {
        purpose: purpose.to_string(),
        truth_plane: None,
        query_id: None,
        query_text_ref,
        normalized_query_text: None,
        query_blake3: blake3::hash(fixture_text.as_bytes()).to_hex().to_string(),
        result_rows: 0,
        row_cap: 100,
        disposition: GeoH7QueryDisposition::Cancelled,
    }
}

fn real_query_receipt(
    purpose: &str,
    query_id: &str,
    normalized_query_text: &str,
    disposition: GeoH7QueryDisposition,
    result_rows: u64,
) -> GeoH7QueryReceipt {
    let query_blake3 = blake3::hash(normalized_query_text.as_bytes())
        .to_hex()
        .to_string();
    GeoH7QueryReceipt {
        purpose: purpose.to_string(),
        truth_plane: None,
        query_id: Some(query_id.to_string()),
        query_text_ref: format!("warehouse_query_history/{query_id}@blake3:{query_blake3}"),
        normalized_query_text: Some(normalized_query_text.to_string()),
        query_blake3,
        result_rows,
        row_cap: 100,
        disposition,
    }
}

fn legal_residual_receipt(truth_plane: GeoTruthPlane, result_rows: u64) -> GeoH7QueryReceipt {
    let (purpose, query_id, sql) = match truth_plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => (
            CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE,
            "synthetic-query-live-non-round-legal-residual",
            "SELECT loan_key, document_id, legal_borough, bbl FROM synthetic_h7_non_round_legal_residual WHERE result_rows > 0",
        ),
        GeoTruthPlane::RoundExactLenderParty => (
            CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE,
            "synthetic-query-live-round-legal-residual",
            "SELECT loan_key, document_id, legal_borough, bbl FROM synthetic_h7_round_lender_legal_residual WHERE result_rows > 0",
        ),
        _ => panic!("not an H7 truth plane"),
    };
    let mut receipt = real_query_receipt(
        purpose,
        query_id,
        sql,
        GeoH7QueryDisposition::Cited,
        result_rows,
    );
    receipt.truth_plane = Some(truth_plane);
    receipt
}

fn install_live_query_receipts(request: &mut GeoH7PopulationRowsRequest) {
    request.provenance.query_receipts = vec![
        real_query_receipt(
            "synthetic_live_raw_property_state_ny_control",
            "synthetic-query-state-control",
            "SELECT property_state, truth_plane, eligible_loans FROM synthetic_h7_raw_property_state_ny_control ORDER BY property_state, truth_plane",
            GeoH7QueryDisposition::Cited,
            7,
        ),
        real_query_receipt(
            "diagnostic_originator_availability_discrepancy",
            "synthetic-query-originator-diagnostic",
            "SELECT truth_plane, raw_originator_available, originator_match_text_available FROM synthetic_h7_originator_availability_diagnostic ORDER BY truth_plane",
            GeoH7QueryDisposition::DiagnosticOnly,
            2,
        ),
        real_query_receipt(
            "diagnostic_round_lender_candidate_aggregation_drift",
            "synthetic-query-round-candidate-diagnostic",
            "SELECT exact_originator_round_loans, candidate_loans, loan_document_pairs FROM synthetic_h7_round_lender_candidate_aggregation",
            GeoH7QueryDisposition::DiagnosticOnly,
            1,
        ),
        real_query_receipt(
            "discarded_cancelled_round_lender_legal_residual",
            "synthetic-query-cancelled-legal-residual",
            "SELECT loan_key, document_id, legal_borough, bbl FROM synthetic_h7_round_lender_legal_residual",
            GeoH7QueryDisposition::Cancelled,
            0,
        ),
    ];
}

fn role_name(role: GeoH7SourceRecordRole) -> &'static str {
    match role {
        GeoH7SourceRecordRole::BridgeLoan => "bridge_loan",
        GeoH7SourceRecordRole::AcrisMaster => "acris_master",
        GeoH7SourceRecordRole::AcrisLegal => "acris_legal",
        GeoH7SourceRecordRole::AcrisParty => "acris_party",
        GeoH7SourceRecordRole::MapplutoCandidate => "mappluto_candidate",
        GeoH7SourceRecordRole::GeocodeDiagnostic => "geocode_diagnostic",
    }
}
