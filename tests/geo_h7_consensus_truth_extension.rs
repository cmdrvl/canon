#![forbid(unsafe_code)]

const H7_CONSENSUS_SQL: &str =
    include_str!("../scripts/geo_measurements/h7_e4_consensus_truth_extension.sql");

fn assert_contains(needle: &str) {
    assert!(
        H7_CONSENSUS_SQL.contains(needle),
        "SQL must contain {needle:?}"
    );
}

fn assert_not_contains(needle: &str) {
    assert!(
        !H7_CONSENSUS_SQL.contains(needle),
        "SQL must not contain {needle:?}"
    );
}

fn offset(needle: &str) -> usize {
    H7_CONSENSUS_SQL
        .find(needle)
        .unwrap_or_else(|| panic!("SQL must contain {needle:?}"))
}

#[test]
fn consensus_extension_is_raw_measurement_not_population_or_solver_claim() {
    assert_contains("h7_e4_consensus_truth_extension_row.v0");
    assert_contains("h7_e4_consensus_document_ambiguous_truth_extension");
    assert_contains("Fewer than five genuinely new subjects is a valid negative finding");

    assert_not_contains("canon_geo_h7_population_rows.v0");
    assert_not_contains("canon.geo.h7_population_rows.v0");
    assert_not_contains("live_complete");
    assert_not_contains("retained_complete");
    assert_not_contains("solver_correct");
}

#[test]
fn controlling_h7_gates_are_preserved_and_planes_are_disjoint() {
    for required in [
        "'3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id",
        "'2026-08-10'::DATE AS acris_release_dt",
        "'NY'::TEXT AS property_state",
        "'nyc_filed_collateral_slice'::TEXT AS collateral_scope",
        "'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT",
        "10000000::NUMBER(38,0) AS round_amount_lattice_cents",
        "45::NUMBER(9,0) AS max_recording_offset_days",
        "MOD(c.amount_cents",
        "'round_exact_lender_party'",
        "'non_round_amount_date_legal_borough'",
        "party.party_name_norm = l.originator_match_text",
        "l.legal_borough = c.filed_borough",
        "amount_cents <> 0",
    ] {
        assert_contains(required);
    }

    assert_contains("distinct_originatorname <= 1");
    assert_contains("distinct_originator_match_text <= 1");
    assert_contains("master_candidates_non_round AS");
    assert_contains("master_candidates_round AS");
}

#[test]
fn candidates_are_constructed_before_legal_truth_and_without_address_channels() {
    let candidates = offset("candidate_documents AS");
    let candidate_counts = offset("candidate_document_counts AS");
    let legal_edges = offset("legal_edges AS");
    let consensus_sets = offset("consensus_truth_bbls AS");

    assert!(candidates < legal_edges);
    assert!(candidate_counts < legal_edges);
    assert!(legal_edges < consensus_sets);

    let candidate_region = &H7_CONSENSUS_SQL[candidates..legal_edges];
    assert!(
        !candidate_region.contains("legal_bbl"),
        "candidate construction must not read legal truth BBLs"
    );

    for forbidden in [
        "PROPERTYADDRESS",
        "PROPERTYCITY",
        "PROPERTYZIP",
        "ADDRESS_1",
        "ADDRESS_2",
        "STREET_NUMBER",
        "STREET_NAME",
        "COUNTY_FIPS",
        "STG_GEO_NYC_MAPPLUTO",
        "MAPPLUTO",
    ] {
        assert_not_contains(forbidden);
    }
}

#[test]
fn consensus_admission_requires_complete_identical_multi_bbl_document_sets() {
    for required in [
        "WHERE l.legal_document_count > 1",
        "candidate_without_legal_document_count = 0",
        "legal_document_count < 2",
        "min_document_bbl_count < 2",
        "distinct_bbl_set_count <> 1",
        "'missing_legal_rows'",
        "'document_bbl_set_not_multi_bbl'",
        "'document_bbl_set_disagreement'",
        "'admitted_consensus_document_ambiguous'",
        "LISTAGG(DISTINCT normalized_legal_bbl, '|')",
        "WITHIN GROUP (ORDER BY normalized_legal_bbl)",
    ] {
        assert_contains(required);
    }
}

#[test]
fn guards_prevent_duplicate_inflation_and_accepted_population_contamination() {
    for guard in [
        "historical_bridge_build_not_retained_in_current_snapshot",
        "2974::NUMBER(38,0) AS expected_h7_eligible_loans",
        "71::NUMBER(38,0) AS expected_h7_accepted_multi_bbl_loans",
        "accepted_h7_multi_bbl_keys AS",
        "truth_plane_summary_missing",
        "eligible_plane_population_count_mismatch",
        "truth_plane_eligible_count_mismatch",
        "truth_plane_multi_bbl_count_mismatch",
        "accepted_71_population_count_mismatch",
        "consensus_subject_row_cap_exceeded",
        "consensus_duplicate_subject",
        "consensus_duplicate_loan_cross_plane",
        "accepted_71_contamination",
        "known_gate_v2_h4_extension_duplicate",
        "admitted_consensus_missing_legal_rows",
        "admitted_consensus_under_document_floor",
        "admitted_consensus_under_bbl_floor",
        "admitted_consensus_bbl_set_disagreement",
        "candidate_master_source_missing",
        "round_party_source_missing",
        "non_round_party_source_leakage",
        "legal_source_missing",
    ] {
        assert_contains(guard);
    }
}

#[test]
fn retention_guard_keeps_historical_pin_and_expected_truth_planes() {
    for required in [
        "expected_truth_planes AS",
        "('non_round_amount_date_legal_borough', 653, 35)",
        "('round_exact_lender_party', 2321, 36)",
        "bridge_pin_stats AS",
        "COUNT(DISTINCT loan_key) AS bridge_distinct_loans",
        "(SELECT bridge_rows FROM bridge_pin_stats) = 0",
        "FROM expected_truth_planes ep",
        "LEFT JOIN eligible_summary e USING (truth_plane)",
    ] {
        assert_contains(required);
    }

    assert_not_contains("ce3953ac-c2d4-4b48-bf02-29f0cf341389");
}

#[test]
fn denominators_are_plane_local_and_accounted() {
    for denominator in [
        "eligible_denominator_reconciles",
        "candidate_denominator_reconciles",
        "legal_denominator_reconciles",
        "ambiguous_consensus_denominator_reconciles",
        "COALESCE(e.eligible_loans, 0) AS eligible_loans",
        "COALESCE(e.eligible_loans, 0) = COALESCE(c.candidate_loans, 0)",
        "SELECT SUM(accepted_h7_multi_bbl_loans) FROM plane_denominators",
        "COALESCE((SELECT SUM(eligible_loans) FROM plane_denominators), 0)",
        "legal_confirmed_candidate_loans",
        "ambiguous_document_identity_loans",
        "candidate_without_legal_loans",
        "no_candidate_loans",
        "admitted_consensus_subjects",
        "rejected_document_bbl_set_disagreement_loans",
    ] {
        assert_contains(denominator);
    }
}

#[test]
fn provenance_uses_available_source_locators_and_no_invented_hashes() {
    for available in [
        "source_row_number",
        "raw_csv_sha256",
        "filename",
        "acris_master_source_record_id",
        "acris_party_source_record_id",
        "acris_legal_source_record_id",
        "bridge_source_record_ids",
        "known_gate_v2_h4_extension_key_dedupe_status",
        "not_available_in_described_warehouse_tables",
    ] {
        assert_contains(available);
    }

    assert_not_contains("BLAKE3");
    assert_not_contains("blake3");
    assert_not_contains("source_hash");
    assert_not_contains("geom_wkt_sha256");
}
