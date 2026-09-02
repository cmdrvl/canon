use canon::geo::{
    CANON_GEO_H7_ACRIS_RELEASE_DT, CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION,
    CANON_GEO_H7_COLLATERAL_SCOPE, CANON_GEO_H7_LENDER_MATCH_TRANSFORM,
    CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
    CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION, CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE,
    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS, GeoH7AssociationPlane, GeoH7CandidateReachStatus,
    GeoH7FiledCountyMapping, GeoH7MapplutoReleasePin, GeoH7PipBlockCountyBoroughEdge,
    GeoH7PipBlockLegalSourceRecord, GeoH7PipBlockPopulationBatchRequest,
    GeoH7PipBlockPopulationRow, GeoH7PlaneDenominator, GeoH7PopulationProvenance,
    GeoH7PopulationScope, GeoH7QueryDisposition, GeoH7QueryReceipt, GeoH7ResultMode,
    GeoH7SourceRecordRole, GeoTruthPlane, h7_population_rows_from_pip_block_population_batch,
    materialize_h7_pip_block_population_batch,
};
use serde_json::{Map, Value};
use std::{fs, process::Command};

const CURRENT_BUILD: &str = "ce3953ac-c2d4-4b48-bf02-29f0cf341389";

#[test]
fn observed_pip_block_batch_materializes_and_content_binds_typed_rows() {
    let batch = observed_batch();
    let rows =
        h7_population_rows_from_pip_block_population_batch(&batch).expect("PIP-block rows adapt");
    let digest = rows
        .provenance
        .observed_payload_blake3
        .as_deref()
        .expect("adapter computes result payload digest");
    assert_eq!(digest.len(), 64);
    assert_eq!(rows.rows.len(), 4);

    let artifact = materialize_h7_pip_block_population_batch(&batch)
        .expect("observed snapshot materializes through production H7 validation");
    assert_eq!(artifact.cases.len(), 4);
    assert_eq!(artifact.summary.materialized_unique_accepted_loans, 2);
    assert_eq!(artifact.summary.solver_population_subjects, 2);
    assert_eq!(artifact.provenance.result_mode, GeoH7ResultMode::Observed);
    assert_eq!(
        artifact.summary.population_scope,
        GeoH7PopulationScope::ObservedSnapshot
    );
    assert!(
        artifact
            .cases
            .iter()
            .flat_map(|case| &case.source_records)
            .any(|record| record.role == GeoH7SourceRecordRole::MapplutoCandidate)
    );
}

#[test]
fn snowflake_uppercase_rows_and_json_string_variants_deserialize() {
    let mut value = serde_json::to_value(observed_batch()).expect("batch serializes");
    let rows = value["staging_rows"]
        .as_array_mut()
        .expect("staging rows array");
    for row in rows {
        let object = row.as_object_mut().expect("row object");
        let canonical = std::mem::take(object);
        let mut uppercase = Map::new();
        for (key, mut field_value) in canonical {
            let warehouse_key = if key == "accepted_truth_binding" {
                "ACCEPTED_TRUTH_QUERY_ID".to_string()
            } else {
                key.to_ascii_uppercase()
            };
            if matches!(
                key.as_str(),
                "filed_counties"
                    | "filed_boroughs"
                    | "filed_county_borough_edges"
                    | "distinct_counts"
                    | "diagnostic_county_fips"
                    | "point_source_record_ids"
                    | "truth_bbls"
                    | "acris_legal_source_records"
                    | "candidate_bbls"
                    | "candidate_source_record_ids"
                    | "candidate_geom_wkt_sha256s"
            ) {
                field_value = Value::String(
                    serde_json::to_string(&field_value).expect("VARIANT JSON serializes"),
                );
            }
            uppercase.insert(warehouse_key, field_value);
        }
        *object = uppercase;
    }
    let parsed: GeoH7PipBlockPopulationBatchRequest =
        serde_json::from_value(value).expect("Snowflake-shaped rows deserialize");
    materialize_h7_pip_block_population_batch(&parsed)
        .expect("Snowflake-shaped rows traverse the adapter");
}

#[test]
fn observed_payload_digest_tampering_is_rejected() {
    let mut batch = observed_batch();
    batch.provenance.observed_payload_blake3 = Some("0".repeat(64));
    let error = h7_population_rows_from_pip_block_population_batch(&batch)
        .expect_err("forged result digest must refuse");
    assert!(error.message.contains("payload digest"));
}

#[test]
fn incomplete_and_misbound_candidate_batches_are_rejected() {
    let mut incomplete = observed_batch();
    incomplete.staging_rows.pop();
    incomplete.provenance.observed_rows -= 1;
    let error = h7_population_rows_from_pip_block_population_batch(&incomplete)
        .expect_err("incomplete release batch must refuse");
    assert!(error.message.contains("incomplete"));

    let mut misbound = observed_batch();
    misbound.staging_rows[0].candidate_source_record_ids[0] =
        "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:26v1:2026-05-01:shoreline_clipped:9999999999:1".to_string();
    let error = h7_population_rows_from_pip_block_population_batch(&misbound)
        .expect_err("candidate locator/BBL mismatch must refuse");
    assert!(error.message.contains("MapPLUTO locator"));

    let mut contradictory_edges = observed_batch();
    contradictory_edges.staging_rows[0].filed_counties = vec!["QUEENS".to_string()];
    let error = h7_population_rows_from_pip_block_population_batch(&contradictory_edges)
        .expect_err("filed arrays cannot contradict accepted edges");
    assert!(error.message.contains("arrays disagree"));

    let mut partial_receipt = observed_batch();
    partial_receipt.provenance.query_receipts[0].result_rows = 3;
    let error = materialize_h7_pip_block_population_batch(&partial_receipt)
        .expect_err("partial diagnostic receipt cannot cover an observed snapshot");
    assert!(error.message.contains("complete typed payload"));
}

#[test]
fn observed_execution_cannot_claim_live_complete_scope() {
    let mut batch = observed_batch();
    batch.population_scope = GeoH7PopulationScope::LiveComplete;
    let error = materialize_h7_pip_block_population_batch(&batch)
        .expect_err("observed result cannot become LiveComplete");
    assert!(
        error.message.contains("scope must agree") || error.message.contains("LiveComplete"),
        "unexpected refusal: {}",
        error.message
    );
}

#[test]
fn pip_block_batch_cli_emits_the_typed_population_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let batch_path = temp.path().join("h7-pip-block-batch.json");
    fs::write(
        &batch_path,
        serde_json::to_vec_pretty(&observed_batch()).expect("batch serializes"),
    )
    .expect("batch fixture writes");
    let output = Command::new(env!("CARGO_BIN_EXE_canon_geo_measurements"))
        .args([
            "materialize-h7-pip-block-batch",
            "--batch",
            batch_path.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("CLI runs");
    assert_eq!(output.status.code(), Some(0));
    let artifact: Value = serde_json::from_slice(&output.stdout).expect("artifact JSON");
    assert_eq!(artifact["version"], "canon_geo_h7_population.v0");
    assert_eq!(artifact["provenance"]["result_mode"], "observed");
    assert_eq!(artifact["summary"]["population_scope"], "observed_snapshot");
    assert_eq!(artifact["summary"]["materialized_unique_accepted_loans"], 2);
}

fn observed_batch() -> GeoH7PipBlockPopulationBatchRequest {
    let query_text = "SELECT bounded_h7_pip_block_population";
    GeoH7PipBlockPopulationBatchRequest {
        version: CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION.to_string(),
        population_scope: GeoH7PopulationScope::ObservedSnapshot,
        provenance: GeoH7PopulationProvenance {
            result_mode: GeoH7ResultMode::Observed,
            as_of: "2026-09-01T21:00:00Z".to_string(),
            acris_release_dt: CANON_GEO_H7_ACRIS_RELEASE_DT.to_string(),
            bridge_build_id: CURRENT_BUILD.to_string(),
            collateral_scope: CANON_GEO_H7_COLLATERAL_SCOPE.to_string(),
            mappluto_releases: vec![mappluto_pin("26v1"), mappluto_pin("26v2")],
            primary_candidate_release: mappluto_pin(CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE),
            amount_cents_quantization: CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION.to_string(),
            round_amount_lattice_cents: CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS,
            lender_match_transform: CANON_GEO_H7_LENDER_MATCH_TRANSFORM.to_string(),
            filed_county_mapping: filed_county_mapping(),
            source_hashes: Vec::new(),
            query_receipts: vec![GeoH7QueryReceipt {
                purpose: "observed_h7_pip_block_population".to_string(),
                truth_plane: None,
                query_id: None,
                query_text_ref: "inline:observed-h7-pip-block-population".to_string(),
                normalized_query_text: Some(query_text.to_string()),
                query_blake3: blake3::hash(query_text.as_bytes()).to_hex().to_string(),
                result_rows: 4,
                row_cap: 10,
                disposition: GeoH7QueryDisposition::DiagnosticOnly,
            }],
            external_receipts: Vec::new(),
            empirical_discrepancies: Vec::new(),
            row_cap: 10,
            observed_rows: 4,
            observed_payload_blake3: None,
        },
        plane_denominators: vec![
            plane_denominator(GeoTruthPlane::NonRoundAmountDateLegalBorough),
            plane_denominator(GeoTruthPlane::RoundExactLenderParty),
        ],
        staging_rows: vec![
            pip_row("non-round-loan", "non-round-doc", "26v1", false),
            pip_row("non-round-loan", "non-round-doc", "26v2", false),
            pip_row("round-loan", "round-doc", "26v1", true),
            pip_row("round-loan", "round-doc", "26v2", true),
        ],
        max_cases: 4,
        max_assignments: 256,
        max_materialized_models: 256,
    }
}

fn pip_row(
    loan_key: &str,
    document_id: &str,
    release: &str,
    round: bool,
) -> GeoH7PipBlockPopulationRow {
    let release_dt = if release == "26v1" {
        "2026-05-01"
    } else {
        "2026-08-01"
    };
    let truth_plane = if round {
        GeoTruthPlane::RoundExactLenderParty
    } else {
        GeoTruthPlane::NonRoundAmountDateLegalBorough
    };
    let amount_cents = if round { 100_000_000 } else { 100_000_001 };
    let truth_bbls = vec!["1000000001".to_string(), "1000000002".to_string()];
    let candidate_source_record_ids = truth_bbls
        .iter()
        .enumerate()
        .map(|(index, bbl)| {
            format!(
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:{release}:{release_dt}:shoreline_clipped:{bbl}:{}",
                index + 1
            )
        })
        .collect();
    GeoH7PipBlockPopulationRow {
        row_contract: "h7_staging_pip_block_population_export_row.v0".to_string(),
        row_kind: "accepted_release_candidate_set".to_string(),
        guard_status: "ok".to_string(),
        refusal_reason: None,
        accepted_truth_binding: "content:blake3:accepted-truth-fixture".to_string(),
        loan_key: loan_key.to_string(),
        truth_plane,
        association_plane: GeoH7AssociationPlane::MultiProperty,
        mappluto_release: release.to_string(),
        mappluto_release_dt: release_dt.to_string(),
        mappluto_variant: "shoreline_clipped".to_string(),
        bridge_build_id: CURRENT_BUILD.to_string(),
        acris_release_dt: CANON_GEO_H7_ACRIS_RELEASE_DT.to_string(),
        collateral_scope: CANON_GEO_H7_COLLATERAL_SCOPE.to_string(),
        accepted_plane_eligible_loans: 1,
        accepted_plane_legal_candidate_loans: 1,
        accepted_plane_legal_confirmed_candidate_loans: 1,
        accepted_plane_accepted_loans: 1,
        accepted_plane_ambiguous_loans: 0,
        accepted_plane_candidate_without_legal_loans: 0,
        accepted_plane_no_candidate_loans: 0,
        accepted_plane_selected_multi_parcel_loans: 1,
        whole_accepted_loans: 2,
        whole_release_rows: 4,
        whole_zero_candidate_release_rows: 0,
        candidate_bbl_count: 2,
        truth_bbl_count: 2,
        reached_truth_bbls: 2,
        reach_status: GeoH7CandidateReachStatus::Full,
        amount_cents,
        originationdate: "2020-01-02".to_string(),
        originatorname: Some("Lender LLC".to_string()),
        originator_match_text: Some("LENDER LLC".to_string()),
        filed_counties: vec!["NEW YORK".to_string()],
        filed_boroughs: vec![1],
        filed_county_borough_edges: vec![GeoH7PipBlockCountyBoroughEdge {
            filed_county: "NEW YORK".to_string(),
            filed_borough: 1,
        }],
        distinct_counts: canon::geo::GeoH7LoanFieldDistinctCounts {
            originatorname: 1,
            originator_match_text: 1,
            originationdate: 1,
            originalloanamount: 1,
            filed_borough: 1,
        },
        diagnostic_county_fips: vec!["36061".to_string()],
        point_source_record_ids: vec![format!(
            "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:{CURRENT_BUILD}:{loan_key}:property-1:link-1"
        )],
        document_id: document_id.to_string(),
        diagnostic_recorded_borough: 1,
        doc_type: "MTGE".to_string(),
        crfn: Some("2020000000001".to_string()),
        document_date: Some("2020-01-02".to_string()),
        recorded_date: Some("2020-01-03".to_string()),
        recording_offset_days: Some(1),
        lender_match_text: round.then(|| "LENDER LLC".to_string()),
        lender_party_type: round.then(|| "2".to_string()),
        acris_master_source_record_id: format!("master:{document_id}"),
        acris_master_raw_csv_sha256: "1".repeat(64),
        acris_master_filename: "master.csv".to_string(),
        acris_party_source_record_id: round.then(|| format!("party:{document_id}")),
        acris_party_raw_csv_sha256: round.then(|| "2".repeat(64)),
        acris_party_filename: round.then(|| "party.csv".to_string()),
        truth_bbls: truth_bbls.clone(),
        acris_legal_source_records: truth_bbls
            .iter()
            .enumerate()
            .map(|(index, bbl)| GeoH7PipBlockLegalSourceRecord {
                source_record_id: format!("legal:{document_id}:{}", index + 1),
                raw_csv_sha256: "3".repeat(64),
                filename: "legal.csv".to_string(),
                legal_bbl: bbl.clone(),
                filed_borough: 1,
            })
            .collect(),
        candidate_bbls: truth_bbls,
        candidate_source_record_ids,
        candidate_geom_wkt_sha256s: vec!["4".repeat(64), "5".repeat(64)],
    }
}

fn plane_denominator(truth_plane: GeoTruthPlane) -> GeoH7PlaneDenominator {
    GeoH7PlaneDenominator {
        truth_plane,
        eligible_loans: 1,
        candidate_loans: 1,
        legal_confirmed_candidate_loans: 1,
        accepted_loans: 1,
        ambiguous_loans: 0,
        candidate_no_legal_confirmation_loans: 0,
        no_candidate_loans: 0,
        selected_multi_parcel_loans: 1,
    }
}

fn mappluto_pin(release: &str) -> GeoH7MapplutoReleasePin {
    GeoH7MapplutoReleasePin {
        release: release.to_string(),
        release_dt: if release == "26v1" {
            "2026-05-01".to_string()
        } else {
            "2026-08-01".to_string()
        },
        variant: "shoreline_clipped".to_string(),
        geometry_contract_version: CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION.to_string(),
    }
}

fn filed_county_mapping() -> Vec<GeoH7FiledCountyMapping> {
    [
        ("BRONX", 2),
        ("BROOKLYN", 3),
        ("KINGS", 3),
        ("MANHATTAN", 1),
        ("NEW YORK", 1),
        ("QUEENS", 4),
        ("RICHMOND", 5),
        ("NY061", 1),
    ]
    .into_iter()
    .map(|(filed_county, acris_borough)| GeoH7FiledCountyMapping {
        filed_county: filed_county.to_string(),
        acris_borough,
    })
    .collect()
}
