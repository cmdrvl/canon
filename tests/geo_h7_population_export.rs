#![forbid(unsafe_code)]

const H7_PIP_BLOCK_POPULATION_EXPORT_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_staging_pip_block_population_export.sql");

fn assert_contains(needle: &str) {
    assert!(
        H7_PIP_BLOCK_POPULATION_EXPORT_SQL.contains(needle),
        "SQL must contain {needle:?}"
    );
}

fn assert_not_contains(needle: &str) {
    assert!(
        !H7_PIP_BLOCK_POPULATION_EXPORT_SQL.contains(needle),
        "SQL must not contain {needle:?}"
    );
}

fn offset(needle: &str) -> usize {
    H7_PIP_BLOCK_POPULATION_EXPORT_SQL
        .find(needle)
        .unwrap_or_else(|| panic!("SQL must contain {needle:?}"))
}

fn count_occurrences(needle: &str) -> usize {
    H7_PIP_BLOCK_POPULATION_EXPORT_SQL
        .match_indices(needle)
        .count()
}

#[test]
fn pip_block_export_has_raw_staging_contract_not_canon_population_contract() {
    assert_contains("h7_staging_pip_block_population_export_row.v0");
    assert_contains("h7_staging_accepted_truth_row.v0");
    assert_contains("01c6c150-0821-a0dc-006c-c703088daab2");
    assert_contains("__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__");
    assert_contains("accepted_truth_query_id_sentinel_unsubstituted");
    assert_contains("RESULT_SCAN('__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__')");
    assert_eq!(
        count_occurrences("RESULT_SCAN("),
        1,
        "export must consume exactly one accepted-truth RESULT_SCAN"
    );

    assert_not_contains("canon_geo_h7_population_rows.v0");
    assert_not_contains("canon.geo.h7_population_rows.v0");
    assert_not_contains("live_complete");
    assert_not_contains("solver_correct");
}

#[test]
fn pip_block_export_is_release_pinned_and_one_row_per_subject_release() {
    assert_contains("('26v1', '2026-05-01', 'shoreline_clipped')");
    assert_contains("('26v2', '2026-08-01', 'shoreline_clipped')");
    assert_contains("71::NUMBER(38,0) AS expected_accepted_loans");
    assert_contains("2::NUMBER(38,0) AS expected_release_count");
    assert_contains("142::NUMBER(38,0) AS expected_export_rows");
    assert_contains("200::NUMBER(38,0) AS export_row_cap");
    assert_contains("CROSS JOIN release_pins pin");
    assert_contains("export_duplicate_subject_release");
    assert_contains("export_row_count_mismatch");
    assert_contains("release_pin_mismatch");
    assert_contains("duplicate_release_pin");
}

#[test]
fn candidate_construction_precedes_truth_flattening_and_is_address_blind() {
    let pip_edges = offset("pip_edges AS");
    let pip_blocks = offset("pip_blocks AS");
    let candidate_edges = offset("candidate_edges AS");
    let truth_edges = offset("truth_edges AS");
    let truth_flatten = offset("LATERAL FLATTEN(input => s.truth_bbls)");

    assert!(
        pip_edges < truth_edges,
        "PIP edges must precede truth flattening"
    );
    assert!(
        pip_blocks < truth_edges,
        "PIP block expansion must precede truth flattening"
    );
    assert!(
        candidate_edges < truth_edges,
        "candidate edges must precede truth flattening"
    );
    assert!(
        truth_edges <= truth_flatten,
        "truth flattening must be isolated to truth_edges"
    );

    let candidate_region = &H7_PIP_BLOCK_POPULATION_EXPORT_SQL[pip_edges..truth_edges];
    assert!(
        !candidate_region.contains("truth_bbl"),
        "candidate construction must not reference accepted truth BBLs"
    );
    for forbidden in [
        "propertyaddress",
        "propertycity",
        "propertyzip",
        "street_number",
        "street_name",
        "address_",
    ] {
        assert!(
            !H7_PIP_BLOCK_POPULATION_EXPORT_SQL
                .to_ascii_lowercase()
                .contains(forbidden),
            "PIP-block export must stay address-blind; found {forbidden}"
        );
    }
}

#[test]
fn zero_candidate_release_rows_are_explicit_and_accounted() {
    assert_contains("COALESCE(cb.candidate_bbl_count, 0) AS candidate_bbl_count");
    assert_contains("COALESCE(cb.candidate_bbls, ARRAY_CONSTRUCT()) AS candidate_bbls");
    assert_contains("candidate_bbl_count = 0");
    assert_contains("zero_candidate_release_rows");
    assert_contains("zero_candidate_subjects");
    assert_contains("zero_candidate_reach_mismatch");
    assert_contains("WHEN COALESCE(h.reached_truth_bbls, 0) = 0 THEN 'none'");
}

#[test]
fn fail_closed_guards_cover_denominators_uniqueness_caps_and_sources() {
    for guard in [
        "accepted_truth_result_empty",
        "accepted_truth_result_exceeds_bound",
        "accepted_truth_result_not_expected_71",
        "accepted_truth_repeats_loan",
        "accepted_truth_contract_mismatch",
        "accepted_truth_bridge_build_mismatch",
        "accepted_truth_acris_release_mismatch",
        "accepted_truth_property_state_mismatch",
        "accepted_truth_non_multi_bbl_row",
        "accepted_truth_bbl_count_mismatch",
        "accepted_truth_missing_bridge_source_records",
        "accepted_truth_insufficient_legal_source_records",
        "export_row_count_mismatch",
        "export_row_count_exceeds_bound",
        "export_duplicate_subject_release",
        "candidate_bbl_count_mismatch",
        "candidate_source_count_mismatch",
        "candidate_bbl_cap_exceeded",
        "reached_truth_accounting_failure",
        "reach_denominator_accounting_failure",
        "whole_reach_denominator_accounting_failure",
    ] {
        assert_contains(guard);
    }

    assert_contains("'2026-08-10'::DATE AS acris_release_dt");
    assert_contains("'NY'::TEXT AS property_state");
    assert_contains("acris_release_dt <> (SELECT acris_release_dt FROM params)");
    assert_contains("property_state_mismatch_rows");

    assert_contains("guard_failure");
    assert_contains("guard_summary");
    assert_contains("WHERE g.guard_status = 'ok'");
}

#[test]
fn export_preserves_only_available_locators_and_digests() {
    assert_contains("EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY");
    assert_contains("EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES");
    assert_contains("source_filename");
    assert_contains("source_row_number");
    assert_contains("geom_wkt_sha256");
    assert_contains("acris_master_raw_csv_sha256");
    assert_contains("acris_legal_source_records");

    assert_not_contains("BLAKE3");
    assert_not_contains("blake3");
    assert_not_contains("source_geom_wkb_sha256");
    assert_not_contains("geom_wgs84_sha256");
    assert_not_contains("source_hash");
}
