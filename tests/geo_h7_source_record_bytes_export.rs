#![forbid(unsafe_code)]

const H7_SOURCE_RECORD_BYTES_EXPORT_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_source_record_bytes_export.sql");

fn sql_lower() -> String {
    H7_SOURCE_RECORD_BYTES_EXPORT_SQL.to_ascii_lowercase()
}

fn assert_contains(needle: &str) {
    assert!(
        H7_SOURCE_RECORD_BYTES_EXPORT_SQL.contains(needle),
        "SQL must contain {needle:?}"
    );
}

fn assert_lower_not_contains(needle: &str) {
    let sql = sql_lower();
    assert!(!sql.contains(needle), "SQL must not contain {needle:?}");
}

fn offset(needle: &str) -> usize {
    H7_SOURCE_RECORD_BYTES_EXPORT_SQL
        .find(needle)
        .unwrap_or_else(|| panic!("SQL must contain {needle:?}"))
}

fn count_occurrences(needle: &str) -> usize {
    H7_SOURCE_RECORD_BYTES_EXPORT_SQL
        .match_indices(needle)
        .count()
}

#[test]
fn source_record_bytes_export_consumes_exactly_one_pip_block_result_scan() {
    assert_contains("h7_staging_source_record_bytes_export_row.v0");
    assert_contains("h7_staging_pip_block_population_export_row.v0");
    assert_contains("01c6c174-0821-aa0e-006c-c703088dc742");
    assert_contains("__BD7BCP_H7_PIP_BLOCK_POPULATION_QUERY_ID__");
    assert_contains("pip_block_population_query_id_sentinel_unsubstituted");
    assert_contains("RESULT_SCAN('__BD7BCP_H7_PIP_BLOCK_POPULATION_QUERY_ID__')");
    assert_eq!(
        count_occurrences("RESULT_SCAN("),
        1,
        "second-stage export must consume exactly one first-stage RESULT_SCAN"
    );
}

#[test]
fn source_payloads_are_derived_canonical_arrays_not_locator_only_hashes() {
    assert_contains("h7_derived_source_record_payload.v0");
    assert_contains("derived_immutable_evidence_record");
    assert_contains("source_record_bytes_base64");
    assert_contains("BASE64_ENCODE(TO_BINARY(payload_json, 'UTF-8'))");
    assert_contains("ARRAY_CONSTRUCT('payload_contract'");
    assert_contains("ARRAY_CONSTRUCT('source_record_class'");
    assert_contains("locator_only_payload_failures");
    assert_contains("payload_utf8_cap_failures");
    assert_contains("row_payload_utf8_cap_failures");
    assert_contains("record_blake3', ''");
    assert_lower_not_contains("original_source_bytes");
    assert_lower_not_contains("source_weight");
    assert_lower_not_contains("evidence_weight");
    assert_lower_not_contains("confidence");
}

#[test]
fn bridge_payload_includes_values_used_by_h7_and_not_address_fields() {
    for required in [
        "ARRAY_CONSTRUCT('originatorname'",
        "ARRAY_CONSTRUCT('originator_match_text'",
        "ARRAY_CONSTRUCT('originationdate'",
        "ARRAY_CONSTRUCT('originalloanamount'",
        "ARRAY_CONSTRUCT('propertystate'",
        "ARRAY_CONSTRUCT('propertycounty'",
        "ARRAY_CONSTRUCT('county_fips'",
        "ARRAY_CONSTRUCT('latitude'",
        "ARRAY_CONSTRUCT('longitude'",
        "ARRAY_CONSTRUCT('loan_property_count'",
    ] {
        assert_contains(required);
    }
    assert_contains("'NY'::TEXT AS expected_property_state");
    assert_contains("lip.propertystate::TEXT AS bridge_property_state");
    assert_contains("input_bridge_property_state_mismatch");
    assert_contains("bridge_property_state_mismatch_rows");
    for forbidden in [
        "propertyaddress",
        "propertycity",
        "propertyzip",
        "property_zipcode",
        "street_number",
        "street_name",
        " address",
    ] {
        assert_lower_not_contains(forbidden);
    }
}

#[test]
fn acris_and_mappluto_payloads_bind_values_locators_and_hashes() {
    for required in [
        "ARRAY_CONSTRUCT('raw_csv_sha256', acris_master_raw_csv_sha256)",
        "ARRAY_CONSTRUCT('filename', acris_master_filename)",
        "ARRAY_CONSTRUCT('document_date'",
        "ARRAY_CONSTRUCT('recorded_date'",
        "ARRAY_CONSTRUCT('lender_match_text', lender_match_text)",
        "ARRAY_CONSTRUCT('lender_party_type', lender_party_type)",
        "ARRAY_CONSTRUCT('legal_bbl', legal_bbl)",
        "derived:canon:h7:acris_legal_edge:",
        "ARRAY_CONSTRUCT('upstream_source_record_id', upstream_source_record_id)",
        "ARRAY_CONSTRUCT('filed_borough', TO_VARCHAR(filed_borough))",
        "ARRAY_CONSTRUCT('bbl_key', p.bbl_key)",
        "ARRAY_CONSTRUCT('source_filename', p.source_filename)",
        "ARRAY_CONSTRUCT('source_row_number'",
        "ARRAY_CONSTRUCT('geom_wkt_sha256', p.geom_wkt_sha256)",
        "ARRAY_CONTAINS(p.geom_wkt_sha256::VARIANT",
        "mappluto_geometry_hash_binding_failures",
    ] {
        assert_contains(required);
    }
    assert_contains("source_hash_format_failures");
    assert_contains("RLIKE '^[0-9a-f]{64}$'");
    assert_contains("'2026-08-10'::DATE AS expected_acris_release_dt");
    assert_contains("input_acris_release_dt_mismatch");
}

#[test]
fn candidate_rejoin_is_before_truth_flattening_and_does_not_seed_from_truth() {
    let candidate_edges = offset("candidate_bbl_edges AS");
    let mappluto_records = offset("mappluto_records AS");
    let legal_entries = offset("acris_legal_entries AS");
    assert!(candidate_edges < mappluto_records);
    assert!(mappluto_records < legal_entries);

    let candidate_region = &H7_SOURCE_RECORD_BYTES_EXPORT_SQL[candidate_edges..legal_entries];
    assert!(
        candidate_region.contains("LATERAL FLATTEN(input => a.candidate_bbls)"),
        "candidate records must flatten first-stage candidates"
    );
    assert!(
        !candidate_region.contains("truth_bbl"),
        "candidate rejoin must not reference truth BBLs"
    );
    assert!(
        !candidate_region.contains("acris_legal_source_records"),
        "candidate rejoin must not seed from ACRIS legal source records"
    );
}

#[test]
fn one_row_per_subject_release_and_closed_guards_are_declared() {
    for required in [
        "71::NUMBER(38,0) AS expected_accepted_loans",
        "2::NUMBER(38,0) AS expected_release_count",
        "142::NUMBER(38,0) AS expected_release_rows",
        "output_row_count_not_142",
        "input_accepted_loan_count_not_71",
        "input_duplicate_subject_release",
        "output_duplicate_subject_release",
        "source_record_id_uniqueness_failures",
        "source_role_coverage_failures",
        "legal_parcel_union_failures",
        "candidate_parcel_union_failures",
        "zero_candidate_mappluto_leakage_rows",
        "nonzero_candidate_missing_mappluto_rows",
        "input_array_mapping_failure",
        "output_source_record_array_failure",
    ] {
        assert_contains(required);
    }
    assert_contains("OR acris_legal_source_record_count = 0");
    assert_lower_not_contains("acris_legal_source_record_count < truth_bbl_count");
}

#[test]
fn accepted_rows_preserve_plane_denominators_for_adapter_drift_checks() {
    for field in [
        "accepted_plane_eligible_loans",
        "accepted_plane_legal_candidate_loans",
        "accepted_plane_legal_confirmed_candidate_loans",
        "accepted_plane_accepted_loans",
        "accepted_plane_ambiguous_loans",
        "accepted_plane_candidate_without_legal_loans",
        "accepted_plane_no_candidate_loans",
        "accepted_plane_selected_multi_parcel_loans",
    ] {
        assert_contains(&format!("r.{field}"));
        assert_contains(&format!("AS {field}"));
    }
}

#[test]
fn legal_and_mappluto_wrappers_name_exactly_one_parcel() {
    assert_contains("'acris_legal'::TEXT AS role");
    assert_contains("'mappluto_candidate'::TEXT AS role");
    assert_contains("ARRAY_CONSTRUCT(legal_bbl) AS parcel_ids");
    assert_contains("ARRAY_CONSTRUCT(c.candidate_bbl) AS parcel_ids");
    assert_contains("single_parcel_wrapper_failures");
    assert_contains("empty_parcel_wrapper_failures");
    assert_contains("candidate_bbl_count = 0 AND mappluto_source_record_count <> 0");
    assert_contains("candidate_bbl_count > 0 AND mappluto_source_record_count = 0");
}
