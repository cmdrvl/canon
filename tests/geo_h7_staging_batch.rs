#![forbid(unsafe_code)]

use assert_cmd::Command;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::geo::{
    CANON_GEO_H7_ACRIS_RELEASE_DT, CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION,
    CANON_GEO_H7_BRIDGE_BUILD_ID, CANON_GEO_H7_COLLATERAL_SCOPE,
    CANON_GEO_H7_LENDER_MATCH_TRANSFORM, CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
    CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE,
    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS,
    CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION, GeoH7AssociationPlane,
    GeoH7BoroughEdge, GeoH7CandidateReachStatus, GeoH7FiledCountyMapping, GeoH7MapplutoReleasePin,
    GeoH7PlaneDenominator, GeoH7PopulationProvenance, GeoH7PopulationScope, GeoH7QueryDisposition,
    GeoH7QueryReceipt, GeoH7ResultMode, GeoH7SourceRecordRole, GeoH7StagingEvidenceRecordRef,
    GeoH7StagingSourceEvidenceRecord, GeoH7StagingSourceRecordBytesBatchRequest,
    GeoH7StagingSourceRecordBytesRow, GeoTruthPlane,
    h7_population_rows_from_staging_source_record_bytes_batch,
    materialize_h7_staging_source_record_bytes_batch,
};
use serde_json::{Map, Value, json};
use std::fs;
use tempfile::tempdir;

const STAGING_ROW_CONTRACT: &str = "h7_staging_source_record_bytes_export_row.v0";
const STAGING_ROW_KIND: &str = "source_record_payload_release_row";
const STAGING_PAYLOAD_CONTRACT: &str = "h7_derived_source_record_payload.v0";
const STAGING_SOURCE_RECORD_CLASS: &str = "derived_immutable_evidence_record";
const H7_STAGING_BATCH_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_staging_source_record_bytes_batch.v0.schema.json");

#[test]
fn staging_batch_materializes_via_existing_h7_population_validator() {
    let batch = staging_batch();
    let rows = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect("staging rows convert to population rows");
    assert_eq!(rows.rows.len(), 4);
    assert_eq!(rows.version, "canon_geo_h7_population_rows.v0");

    let artifact = materialize_h7_staging_source_record_bytes_batch(&batch)
        .expect("staging batch materializes");
    assert_eq!(artifact.version, CANON_GEO_H7_POPULATION_VERSION);
    assert_eq!(artifact.summary.source_rows, 4);
    assert_eq!(artifact.summary.materialized_unique_accepted_loans, 2);
    assert_eq!(artifact.summary.solver_population_subjects, 2);
}

#[test]
fn staging_batch_cli_emits_canonical_h7_population_artifact() {
    let temp = tempdir().expect("tempdir");
    let batch_path = temp.path().join("h7-staging-batch.json");
    fs::write(
        &batch_path,
        serde_json::to_vec_pretty(&staging_batch()).expect("batch serializes"),
    )
    .expect("write batch");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "geo",
            "materialize-h7-staging-batch",
            "--batch",
            batch_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifact: Value = serde_json::from_slice(&output).expect("artifact JSON");
    assert_eq!(artifact["version"], CANON_GEO_H7_POPULATION_VERSION);
    assert_eq!(artifact["summary"]["source_rows"], 4);
    assert_eq!(artifact["population"]["cases"].as_array().unwrap().len(), 2);
}

#[test]
fn staging_batch_accepts_uppercase_snowflake_row_keys_and_source_record_bytes() {
    let mut value = serde_json::to_value(staging_batch()).expect("batch serializes");
    uppercase_staging_row_keys(&mut value);
    let first_source_record = &mut value["staging_rows"][0]["SOURCE_RECORDS"][0];
    first_source_record["source_record"]
        .as_object_mut()
        .expect("source_record object")
        .remove("record_blake3");

    let batch: GeoH7StagingSourceRecordBytesBatchRequest =
        serde_json::from_value(value).expect("uppercase Snowflake rows parse");
    let artifact = materialize_h7_staging_source_record_bytes_batch(&batch)
        .expect("uppercase staging batch materializes");
    let artifact_value = serde_json::to_value(artifact).expect("artifact serializes");
    assert_eq!(artifact_value["summary"]["source_rows"], 4);
    assert!(
        !artifact_value
            .to_string()
            .contains("source_record_bytes_base64")
    );
}

#[test]
fn staging_batch_schema_declares_runtime_handoff_shape() {
    let schema: Value = serde_json::from_str(H7_STAGING_BATCH_SCHEMA).expect("schema JSON");
    assert_eq!(
        schema["title"],
        "canon.geo.h7_staging_source_record_bytes_batch.v0"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION
    );
    assert_eq!(schema["properties"]["max_cases"]["minimum"], 1);

    let staging_row_props = schema["$defs"]["staging_row"]["properties"]
        .as_object()
        .expect("staging row properties");
    for key in [
        "ROW_CONTRACT",
        "SOURCE_RECORDS",
        "PROPERTY_STATE",
        "TRUTH_PARCELS",
        "ACCEPTED_PLANE_ACCEPTED_LOANS",
        "ACCEPTED_PLANE_SELECTED_MULTI_PARCEL_LOANS",
    ] {
        assert!(
            staging_row_props.contains_key(key),
            "schema missing Snowflake key {key}"
        );
    }
    assert_eq!(
        schema["$defs"]["nullable_property_state"]["oneOf"][0]["const"],
        "NY"
    );
    assert_eq!(
        schema["$defs"]["nullable_truth_parcels"]["oneOf"][0]["minItems"],
        2
    );
    assert_eq!(
        schema["$defs"]["staging_source_evidence_record"]["properties"]["source_record_bytes_base64"]
            ["type"],
        "string"
    );

    let instance = serde_json::to_value(staging_batch()).expect("batch serializes");
    let top_props = schema["properties"]
        .as_object()
        .expect("top-level properties");
    for key in instance.as_object().expect("batch object").keys() {
        assert!(
            top_props.contains_key(key),
            "schema missing batch key {key}"
        );
    }
}

#[test]
fn staging_batch_rejects_uppercase_snowflake_guard_rows() {
    let mut value = serde_json::to_value(staging_batch()).expect("batch serializes");
    uppercase_staging_row_keys(&mut value);
    value["staging_rows"][0]["ROW_KIND"] = json!("guard_failure");
    value["staging_rows"][0]["GUARD_STATUS"] = json!("refused");
    value["staging_rows"][0]["REFUSAL_REASON"] = json!("synthetic_guard_failure");

    let batch: GeoH7StagingSourceRecordBytesBatchRequest =
        serde_json::from_value(value).expect("uppercase Snowflake guard row parses");
    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("uppercase guard row is not a payload row");
    assert!(
        error
            .message
            .contains("only accepts source-record payload release rows")
            || error.message.contains("must have an ok upstream guard")
    );
}

#[test]
fn staging_batch_rejects_guard_rows_even_with_materializer_fields_present() {
    let mut batch = staging_batch();
    batch.staging_rows[0].row_kind = "guard_failure".to_string();
    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("guard rows are not pass-through rows");
    assert!(
        error
            .message
            .contains("only accepts source-record payload release rows")
    );
}

#[test]
fn staging_batch_rejects_arbitrary_base64_bytes_under_valid_locator() {
    let mut batch = staging_batch();
    batch.staging_rows[0].source_records.as_mut().unwrap()[0]
        .source_record
        .record_blake3
        .clear();
    batch.staging_rows[0].source_records.as_mut().unwrap()[0].source_record_bytes_base64 =
        BASE64_STANDARD.encode(b"warehouse-derived-source-record-bytes");

    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("arbitrary bytes are not a derived Canon payload");
    assert!(
        error.message.contains("derived source-record payload")
            && (error.message.contains("not JSON")
                || error.message.contains("key/value-pair array"))
    );
}

#[test]
fn staging_batch_rejects_mismatched_role_payload_and_wrapper_parcel() {
    let mut batch = staging_batch();
    let legal_record = batch.staging_rows[0]
        .source_records
        .as_mut()
        .expect("source records")
        .iter_mut()
        .find(|record| record.role == GeoH7SourceRecordRole::AcrisLegal)
        .expect("legal record");
    legal_record.source_record_bytes_base64 = source_record_payload_base64(
        legal_record.role,
        &legal_record.source_record.source_record_id,
        &legal_record.source_record.source_vintage,
        Some(("legal_bbl", "different-bbl")),
    );

    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("payload legal_bbl must bind to wrapper parcel");
    assert!(
        error
            .message
            .contains("derived source-record payload does not match its wrapper")
    );
    assert_eq!(
        error.detail.get("key").map(String::as_str),
        Some("legal_bbl")
    );

    let mut batch = staging_batch();
    let mappluto_record = batch.staging_rows[0]
        .source_records
        .as_mut()
        .expect("source records")
        .iter_mut()
        .find(|record| record.role == GeoH7SourceRecordRole::MapplutoCandidate)
        .expect("mappluto record");
    mappluto_record.source_record_bytes_base64 = source_record_payload_base64(
        mappluto_record.role,
        &mappluto_record.source_record.source_record_id,
        &mappluto_record.source_record.source_vintage,
        Some(("bbl_key", "different-bbl")),
    );

    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("payload bbl_key must bind to wrapper parcel");
    assert!(
        error
            .message
            .contains("derived source-record payload does not match its wrapper")
    );
    assert_eq!(error.detail.get("key").map(String::as_str), Some("bbl_key"));
}

#[test]
fn staging_batch_rejects_malformed_derived_payload_shape() {
    let mut batch = staging_batch();
    batch.staging_rows[0].source_records.as_mut().unwrap()[0].source_record_bytes_base64 =
        BASE64_STANDARD.encode(
            serde_json::to_vec(&json!([
                ["payload_contract", STAGING_PAYLOAD_CONTRACT],
                ["payload_contract", STAGING_PAYLOAD_CONTRACT],
            ]))
            .expect("payload JSON"),
        );
    let error = h7_population_rows_from_staging_source_record_bytes_batch(&batch)
        .expect_err("duplicate payload keys rejected");
    assert!(error.message.contains("repeats a key"));
}

#[test]
fn staging_batch_rejects_single_row_incomplete_batches() {
    let mut batch = staging_batch();
    batch.staging_rows.truncate(1);
    batch.staging_rows[0].whole_accepted_loans = Some(1);
    batch.staging_rows[0].whole_release_rows = Some(1);
    let error = materialize_h7_staging_source_record_bytes_batch(&batch)
        .expect_err("single staging row cannot become a complete H7 request");
    assert!(
        error
            .message
            .contains("accepted loans require exactly one row")
            || error
                .message
                .contains("must keep both controlling truth planes")
    );
}

#[test]
fn staging_batch_rejects_mixed_metadata_and_denominator_drift() {
    let mut mixed = staging_batch();
    mixed.staging_rows[1].accepted_truth_query_id = Some("different-truth-query".to_string());
    let error = h7_population_rows_from_staging_source_record_bytes_batch(&mixed)
        .expect_err("mixed query metadata rejected");
    assert!(error.message.contains("conflicting batch metadata"));

    let mut drifted = staging_batch();
    drifted.staging_rows[0].accepted_plane_accepted_loans = Some(999);
    let error = h7_population_rows_from_staging_source_record_bytes_batch(&drifted)
        .expect_err("staging denominators must match operator denominators");
    assert!(
        error
            .message
            .contains("staging denominator fields disagree")
    );
}

fn staging_batch() -> GeoH7StagingSourceRecordBytesBatchRequest {
    GeoH7StagingSourceRecordBytesBatchRequest {
        version: CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION.to_string(),
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
            query_receipts: vec![receipt("fixture_h7_staging_batch_replay", 4)],
            external_receipts: Vec::new(),
            empirical_discrepancies: Vec::new(),
            row_cap: 10,
            observed_rows: 0,
        },
        plane_denominators: vec![
            plane_denominator(GeoTruthPlane::NonRoundAmountDateLegalBorough),
            plane_denominator(GeoTruthPlane::RoundExactLenderParty),
        ],
        staging_rows: vec![
            staging_row("loan-nonround", "doc-nonround", "26v1", false),
            staging_row("loan-nonround", "doc-nonround", "26v2", false),
            staging_row("loan-round", "doc-round", "26v1", true),
            staging_row("loan-round", "doc-round", "26v2", true),
        ],
        max_cases: 8,
        max_assignments: 64,
        max_materialized_models: 64,
    }
}

fn staging_row(
    loan_key: &str,
    document_id: &str,
    release: &str,
    round: bool,
) -> GeoH7StagingSourceRecordBytesRow {
    let truth_plane = if round {
        GeoTruthPlane::RoundExactLenderParty
    } else {
        GeoTruthPlane::NonRoundAmountDateLegalBorough
    };
    let denominator = plane_denominator(truth_plane);
    let candidate_release = mappluto_pin(release);
    let truth_parcels = vec![format!("{loan_key}-bbl-1"), format!("{loan_key}-bbl-2")];
    let candidate_parcels = truth_parcels.clone();
    let mut source_records = vec![
        source_record(
            GeoH7SourceRecordRole::BridgeLoan,
            &format!("{loan_key}:bridge"),
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
            &format!("{document_id}:legal-1"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[truth_parcels[0].as_str()],
        ),
        source_record(
            GeoH7SourceRecordRole::AcrisLegal,
            &format!("{document_id}:legal-2"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[truth_parcels[1].as_str()],
        ),
        source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:candidate-1"),
            candidate_release.release_dt.as_str(),
            &[candidate_parcels[0].as_str()],
        ),
        source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:candidate-2"),
            candidate_release.release_dt.as_str(),
            &[candidate_parcels[1].as_str()],
        ),
    ];
    if round {
        source_records.push(source_record(
            GeoH7SourceRecordRole::AcrisParty,
            &format!("{document_id}:party"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[],
        ));
    }

    GeoH7StagingSourceRecordBytesRow {
        row_contract: STAGING_ROW_CONTRACT.to_string(),
        row_kind: STAGING_ROW_KIND.to_string(),
        guard_status: "ok".to_string(),
        refusal_reason: None,
        pip_block_population_query_id: "01c6c150-0821-a0dc-006c-c703088daab2".to_string(),
        payload_contract: STAGING_PAYLOAD_CONTRACT.to_string(),
        source_record_class: STAGING_SOURCE_RECORD_CLASS.to_string(),
        accepted_truth_query_id: Some("01c6bfda-0821-a0dc-006c-c703088d161e".to_string()),
        loan_key: Some(loan_key.to_string()),
        document_id: Some(document_id.to_string()),
        truth_plane: Some(truth_plane),
        association_plane: Some(GeoH7AssociationPlane::MultiProperty),
        mappluto_release: Some(candidate_release.release.clone()),
        mappluto_release_dt: Some(candidate_release.release_dt.clone()),
        mappluto_variant: Some(candidate_release.variant.clone()),
        candidate_release: Some(candidate_release),
        property_state: Some("NY".to_string()),
        filed_county: Some("KINGS".to_string()),
        filed_borough: Some(3),
        legal_borough: Some(3),
        accepted_borough_edges: Some(vec![GeoH7BoroughEdge {
            filed_county: "KINGS".to_string(),
            filed_borough: 3,
            legal_borough: 3,
        }]),
        geocoded_county_fips: None,
        doc_type: Some(if round { "MMTG" } else { "MTGE" }.to_string()),
        originationdate: Some("2025-01-15".to_string()),
        amount_cents: Some(if round { 50_000_000 } else { 12_345_678 }),
        is_round_100k_lattice: Some(round),
        originatorname: round.then(|| "Acme Bank".to_string()),
        originator_match_text: round.then(|| "ACME BANK".to_string()),
        lender_match_text: round.then(|| "ACME BANK".to_string()),
        lender_party_type: round.then(|| "1".to_string()),
        loan_field_distinct_counts: Some(canon::geo::GeoH7LoanFieldDistinctCounts {
            originatorname: if round { 1 } else { 0 },
            originator_match_text: if round { 1 } else { 0 },
            originationdate: 1,
            originalloanamount: 1,
            filed_borough: 1,
        }),
        truth_parcels: Some(truth_parcels),
        candidate_parcels: Some(candidate_parcels),
        reach_status: Some(GeoH7CandidateReachStatus::Full),
        reach_reason: Some(
            "pip_block_candidate_release_scored_against_same_accepted_h7_truth".to_string(),
        ),
        source_records: Some(source_records.clone()),
        source_record_count: Some(source_records.len() as u64),
        bridge_source_record_count: Some(role_count(
            &source_records,
            GeoH7SourceRecordRole::BridgeLoan,
        )),
        acris_master_source_record_count: Some(role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisMaster,
        )),
        acris_party_source_record_count: Some(role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisParty,
        )),
        acris_legal_source_record_count: Some(role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisLegal,
        )),
        mappluto_source_record_count: Some(role_count(
            &source_records,
            GeoH7SourceRecordRole::MapplutoCandidate,
        )),
        min_source_record_payload_utf8_bytes: None,
        max_source_record_payload_utf8_bytes: None,
        total_source_record_payload_utf8_bytes: None,
        max_source_record_payload_base64_chars: None,
        candidate_bbl_count: Some(2),
        truth_bbl_count: Some(2),
        reached_truth_bbls: Some(2),
        whole_accepted_loans: Some(2),
        whole_release_rows: Some(4),
        whole_zero_candidate_release_rows: Some(0),
        accepted_plane_eligible_loans: Some(denominator.eligible_loans),
        accepted_plane_legal_candidate_loans: Some(denominator.candidate_loans),
        accepted_plane_legal_confirmed_candidate_loans: Some(
            denominator.legal_confirmed_candidate_loans,
        ),
        accepted_plane_accepted_loans: Some(denominator.accepted_loans),
        accepted_plane_ambiguous_loans: Some(denominator.ambiguous_loans),
        accepted_plane_candidate_without_legal_loans: Some(
            denominator.candidate_no_legal_confirmation_loans,
        ),
        accepted_plane_no_candidate_loans: Some(denominator.no_candidate_loans),
        accepted_plane_selected_multi_parcel_loans: Some(denominator.selected_multi_parcel_loans),
    }
}

fn plane_denominator(truth_plane: GeoTruthPlane) -> GeoH7PlaneDenominator {
    match truth_plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => GeoH7PlaneDenominator {
            truth_plane,
            eligible_loans: 653,
            candidate_loans: 262,
            legal_confirmed_candidate_loans: 221,
            accepted_loans: 172,
            ambiguous_loans: 49,
            candidate_no_legal_confirmation_loans: 41,
            no_candidate_loans: 391,
            selected_multi_parcel_loans: 1,
        },
        GeoTruthPlane::RoundExactLenderParty => GeoH7PlaneDenominator {
            truth_plane,
            eligible_loans: 2321,
            candidate_loans: 182,
            legal_confirmed_candidate_loans: 179,
            accepted_loans: 149,
            ambiguous_loans: 30,
            candidate_no_legal_confirmation_loans: 3,
            no_candidate_loans: 2139,
            selected_multi_parcel_loans: 1,
        },
        _ => panic!("not an H7 truth plane"),
    }
}

fn mappluto_pin(release: &str) -> GeoH7MapplutoReleasePin {
    GeoH7MapplutoReleasePin {
        release: release.to_string(),
        release_dt: match release {
            "26v1" => "2026-05-01",
            "26v2" => "2026-08-01",
            _ => "2026-12-01",
        }
        .to_string(),
        variant: "shoreline_clipped".to_string(),
        geometry_contract_version: CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION.to_string(),
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
) -> GeoH7StagingSourceEvidenceRecord {
    let parcel_key = match role {
        GeoH7SourceRecordRole::AcrisLegal => parcel_ids
            .first()
            .map(|parcel_id| ("legal_bbl", *parcel_id)),
        GeoH7SourceRecordRole::MapplutoCandidate => {
            parcel_ids.first().map(|parcel_id| ("bbl_key", *parcel_id))
        }
        _ => None,
    };
    let source_record_bytes_base64 =
        source_record_payload_base64(role, source_record_id, source_vintage, parcel_key);
    let record_blake3 = blake3::hash(
        BASE64_STANDARD
            .decode(source_record_bytes_base64.as_bytes())
            .expect("canonical payload base64")
            .as_slice(),
    )
    .to_hex()
    .to_string();
    GeoH7StagingSourceEvidenceRecord {
        role,
        parcel_ids: parcel_ids
            .iter()
            .map(|parcel_id| (*parcel_id).to_string())
            .collect(),
        source_record: GeoH7StagingEvidenceRecordRef {
            source_record_id: source_record_id.to_string(),
            source_vintage: source_vintage.to_string(),
            record_blake3,
        },
        source_record_bytes_base64,
    }
}

fn source_record_payload_base64(
    role: GeoH7SourceRecordRole,
    source_record_id: &str,
    source_vintage: &str,
    parcel_key: Option<(&str, &str)>,
) -> String {
    let mut pairs = vec![
        ["payload_contract", STAGING_PAYLOAD_CONTRACT],
        ["source_record_class", STAGING_SOURCE_RECORD_CLASS],
        ["role", role_name(role)],
        ["source_record_id", source_record_id],
        ["source_vintage", source_vintage],
    ];
    if let Some((key, value)) = parcel_key {
        pairs.push([key, value]);
    }
    pairs.sort_by(|left, right| left[0].cmp(right[0]));
    BASE64_STANDARD.encode(serde_json::to_vec(&pairs).expect("payload JSON"))
}

fn role_count(records: &[GeoH7StagingSourceEvidenceRecord], role: GeoH7SourceRecordRole) -> u64 {
    records.iter().filter(|record| record.role == role).count() as u64
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

fn receipt(purpose: &str, result_rows: u64) -> GeoH7QueryReceipt {
    let query_text_ref = format!("fixture:h7-staging-batch:{purpose}");
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

fn uppercase_staging_row_keys(value: &mut Value) {
    for row in value["staging_rows"]
        .as_array_mut()
        .expect("staging rows array")
    {
        let object = row.as_object_mut().expect("staging row object");
        let original = std::mem::take(object);
        let mut uppercase = Map::new();
        for (key, value) in original {
            uppercase.insert(key.to_ascii_uppercase(), value);
        }
        *object = uppercase;
    }
}
