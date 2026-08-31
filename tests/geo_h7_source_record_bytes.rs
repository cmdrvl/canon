#![forbid(unsafe_code)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::geo::evidence::GeoEvidenceRecordRef;
use canon::geo::{
    CANON_GEO_H7_ACRIS_RELEASE_DT, CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION,
    CANON_GEO_H7_BRIDGE_BUILD_ID, CANON_GEO_H7_COLLATERAL_SCOPE,
    CANON_GEO_H7_LENDER_MATCH_TRANSFORM, CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
    CANON_GEO_H7_POPULATION_ROWS_VERSION, CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE,
    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS, GeoH7AssociationPlane, GeoH7BoroughEdge,
    GeoH7CandidateReachStatus, GeoH7EmpiricalDiscrepancy, GeoH7EmpiricalDiscrepancyStatus,
    GeoH7ExternalReceiptKind, GeoH7ExternalReceiptRef, GeoH7FiledCountyMapping,
    GeoH7LoanFieldDistinctCounts, GeoH7MapplutoReleasePin, GeoH7PlaneDenominator,
    GeoH7PopulationProvenance, GeoH7PopulationRowsRequest, GeoH7PopulationScope,
    GeoH7PopulationWarehouseRow, GeoH7QueryDisposition, GeoH7QueryReceipt, GeoH7ResultMode,
    GeoH7SourceEvidenceRecord, GeoH7SourceRecordRole, GeoTruthPlane,
    materialize_h7_population_rows,
};
use serde_json::{Value, json};

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
                    archived_measurement:
                        "G7 archived availability: non-round 605/653, round 2173/2321"
                            .to_string(),
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

fn request_value_with_source_bytes() -> Value {
    let mut value = serde_json::to_value(base_request()).expect("fixture serializes");
    for row in value["rows"]
        .as_array_mut()
        .expect("fixture rows are an array")
    {
        for record in row["source_records"]
            .as_array_mut()
            .expect("fixture source records are an array")
        {
            let role = record["role"].as_str().expect("role").to_string();
            let source_record = record
                .get_mut("source_record")
                .expect("source_record")
                .as_object_mut()
                .expect("source_record object");
            let source_record_id = source_record["source_record_id"]
                .as_str()
                .expect("source_record_id")
                .to_string();
            let source_vintage = source_record["source_vintage"]
                .as_str()
                .expect("source_vintage")
                .to_string();
            source_record.remove("record_blake3");
            let bytes = source_record_bytes(&role, &source_record_id, &source_vintage);
            record["source_record_bytes_base64"] = json!(BASE64_STANDARD.encode(bytes));
        }
    }
    value
}

fn source_record_bytes(role: &str, source_record_id: &str, source_vintage: &str) -> Vec<u8> {
    format!("exact-h7-source-record-bytes\0{role}\0{source_record_id}\0{source_vintage}")
        .into_bytes()
}

fn source_record_digest(role: &str, source_record_id: &str, source_vintage: &str) -> String {
    blake3::hash(&source_record_bytes(role, source_record_id, source_vintage))
        .to_hex()
        .to_string()
}

fn source_record_digest_from_value(record: &Value) -> &str {
    record
        .get("source_record")
        .and_then(|value| value.get("record_blake3"))
        .and_then(Value::as_str)
        .expect("source record digest")
}

fn first_source_record_mut(value: &mut Value) -> &mut Value {
    &mut value["rows"][0]["source_records"][0]
}

fn artifact_source_record<'a>(artifact: &'a Value, source_record_id: &str) -> Option<&'a Value> {
    artifact["cases"]
        .as_array()?
        .iter()
        .flat_map(|case| case["source_records"].as_array().into_iter().flatten())
        .find(|record| {
            record["source_record"]["source_record_id"].as_str() == Some(source_record_id)
        })
}

#[test]
fn h7_source_record_bytes_compute_blake3_and_are_canonicalized_out() {
    let value = request_value_with_source_bytes();
    let request: GeoH7PopulationRowsRequest =
        serde_json::from_value(value).expect("source bytes parse");
    let target = &request.rows[0].source_records[0];
    let source_record_id = target.source_record.source_record_id.as_str();
    let expected = source_record_digest(
        "bridge_loan",
        source_record_id,
        CANON_GEO_H7_BRIDGE_BUILD_ID,
    );
    assert_eq!(target.source_record.record_blake3, expected);
    assert_ne!(
        target.source_record.record_blake3,
        blake3::hash(source_record_id.as_bytes())
            .to_hex()
            .to_string()
    );

    let artifact = materialize_h7_population_rows(&request).expect("source bytes materialize");
    let artifact_value = serde_json::to_value(&artifact).expect("artifact serializes");
    assert!(
        artifact_value
            .to_string()
            .find("source_record_bytes_base64")
            .is_none()
    );
    let emitted = artifact_source_record(&artifact_value, source_record_id)
        .expect("computed source record is emitted");
    assert_eq!(source_record_digest_from_value(emitted), expected);
}

#[test]
fn h7_source_record_byte_changes_change_digest_under_same_locator() {
    let original: GeoH7PopulationRowsRequest =
        serde_json::from_value(request_value_with_source_bytes()).expect("source bytes parse");
    let source_record_id = original.rows[0].source_records[0]
        .source_record
        .source_record_id
        .clone();
    let original_digest = original.rows[0].source_records[0]
        .source_record
        .record_blake3
        .clone();

    let mut changed_value = request_value_with_source_bytes();
    first_source_record_mut(&mut changed_value)["source_record_bytes_base64"] =
        json!(BASE64_STANDARD.encode(b"different exact source bytes for same locator"));
    let changed: GeoH7PopulationRowsRequest =
        serde_json::from_value(changed_value).expect("changed source bytes parse");
    assert_eq!(
        changed.rows[0].source_records[0]
            .source_record
            .source_record_id,
        source_record_id
    );
    assert_ne!(
        changed.rows[0].source_records[0]
            .source_record
            .record_blake3,
        original_digest
    );
}

#[test]
fn h7_source_record_bytes_accept_empty_or_matching_declared_digest() {
    let mut empty_digest_value = request_value_with_source_bytes();
    let empty_record = first_source_record_mut(&mut empty_digest_value);
    empty_record["source_record"]["record_blake3"] = json!("");
    serde_json::from_value::<GeoH7PopulationRowsRequest>(empty_digest_value)
        .expect("empty digest is allowed only when bytes are present");

    let mut matching_digest_value = request_value_with_source_bytes();
    let matching_record = first_source_record_mut(&mut matching_digest_value);
    let role = matching_record["role"].as_str().expect("role");
    let source_record_id = matching_record["source_record"]["source_record_id"]
        .as_str()
        .expect("source_record_id");
    let source_vintage = matching_record["source_record"]["source_vintage"]
        .as_str()
        .expect("source_vintage");
    matching_record["source_record"]["record_blake3"] =
        json!(source_record_digest(role, source_record_id, source_vintage));
    serde_json::from_value::<GeoH7PopulationRowsRequest>(matching_digest_value)
        .expect("matching digest and bytes are accepted");
}

#[test]
fn h7_source_record_bytes_reject_locator_hash_mismatch() {
    let mut value = request_value_with_source_bytes();
    let record = first_source_record_mut(&mut value);
    let source_record_id = record["source_record"]["source_record_id"]
        .as_str()
        .expect("source_record_id");
    record["source_record"]["record_blake3"] = json!(
        blake3::hash(source_record_id.as_bytes())
            .to_hex()
            .to_string()
    );
    let error = serde_json::from_value::<GeoH7PopulationRowsRequest>(value)
        .expect_err("locator-derived digest must not agree with source bytes");
    assert!(error.to_string().contains("does not match"));
}

#[test]
fn h7_source_record_bytes_reject_invalid_empty_and_missing_inputs() {
    let mut invalid_base64 = request_value_with_source_bytes();
    first_source_record_mut(&mut invalid_base64)["source_record_bytes_base64"] = json!("Zg");
    let error = serde_json::from_value::<GeoH7PopulationRowsRequest>(invalid_base64)
        .expect_err("noncanonical base64 must reject");
    assert!(error.to_string().contains("base64"));

    let mut empty_bytes = request_value_with_source_bytes();
    first_source_record_mut(&mut empty_bytes)["source_record_bytes_base64"] = json!("");
    let error = serde_json::from_value::<GeoH7PopulationRowsRequest>(empty_bytes)
        .expect_err("empty source bytes must reject");
    assert!(error.to_string().contains("empty source record"));

    let mut missing_both = request_value_with_source_bytes();
    let record = first_source_record_mut(&mut missing_both);
    record
        .as_object_mut()
        .expect("source evidence object")
        .remove("source_record_bytes_base64");
    let error = serde_json::from_value::<GeoH7PopulationRowsRequest>(missing_both)
        .expect_err("digest or source bytes are required");
    assert!(error.to_string().contains("requires record_blake3"));

    let mut null_bytes = request_value_with_source_bytes();
    first_source_record_mut(&mut null_bytes)["source_record_bytes_base64"] = Value::Null;
    let error = serde_json::from_value::<GeoH7PopulationRowsRequest>(null_bytes)
        .expect_err("explicit null source bytes must not be treated as an omitted field");
    assert!(error.to_string().contains("expected a string"));
}

#[test]
fn h7_digest_only_source_record_path_is_preserved() {
    let request = base_request();
    let digest = request.rows[0].source_records[0]
        .source_record
        .record_blake3
        .clone();
    let artifact = materialize_h7_population_rows(&request).expect("digest-only rows materialize");
    assert!(
        artifact
            .cases
            .iter()
            .flat_map(|case| case.source_records.iter())
            .any(|record| record.source_record.record_blake3 == digest)
    );
}

#[test]
fn h7_population_rows_schema_declares_source_record_bytes_input_path() {
    let schema: Value = serde_json::from_str(include_str!(
        "../schemas/canon.geo.h7_population_rows.v0.schema.json"
    ))
    .expect("schema parses");
    assert!(
        schema
            .pointer("/$defs/source_evidence_record/properties/source_record_bytes_base64")
            .is_some()
    );
    let source_record_required = schema
        .pointer("/$defs/evidence_record_ref/required")
        .and_then(Value::as_array)
        .expect("source record required fields");
    assert!(
        !source_record_required
            .iter()
            .any(|field| field.as_str() == Some("record_blake3"))
    );
    assert!(
        schema
            .pointer("/$defs/source_evidence_record/allOf/0/anyOf/1/required/0")
            .and_then(Value::as_str)
            == Some("source_record_bytes_base64")
    );
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
        loan_field_distinct_counts: GeoH7LoanFieldDistinctCounts {
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
