-- Appendix H.7 PIP-block candidate population export over accepted truth.
--
-- This single SELECT consumes one successful h7_staging_accepted_truth_row.v0
-- RESULT_SCAN, then builds an address-blind point-in-polygon candidate relation
-- before flattening accepted ACRIS truth. For each accepted multi-BBL loan it
-- emits exactly one bounded row for each pinned MapPLUTO release:
--
--   * 26v1 / 2026-05-01 / shoreline_clipped
--   * 26v2 / 2026-08-01 / shoreline_clipped
--
-- Candidate construction uses only the bridge loan/property point coordinates
-- and release-pinned MapPLUTO geometry. It does not use address fields, ACRIS
-- truth BBLs, geocoded county truth, or solver output. The row contract below
-- is a raw staging handoff, not the typed Canon H7 population-row contract.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'
--
-- Expected positive control: accepted truth query 01c6bfda-0821-a0dc-006c-c703088d161e
-- exported 71 accepted multi-BBL loan rows, so this query should emit 142
-- release rows when the RESULT_SCAN is still available and the warehouse
-- snapshots remain unchanged.
--
-- Positive compact export receipt:
-- * 01c6c150-0821-a0dc-006c-c703088daab2, 39.975s, 142 rows
--   produced from accepted truth query 01c6bfda-0821-a0dc-006c-c703088d161e;
--   readback found 71 accepted loans, 0 guard rows, and 2 explicit
--   zero-candidate release rows.
--
-- Discarded attempt retained as context only:
-- * 01c6c14b-0821-a0dc-006c-c703088da9fa, 45s: first full export
--   shape with nested per-candidate OBJECT payloads cancelled. This file now
--   preserves compact candidate BBL, source-record-id, and geom_wkt_sha256
--   arrays instead of object payloads.

WITH
params AS (
  SELECT
    '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'::TEXT
      AS accepted_truth_query_id,
    'h7_staging_accepted_truth_row.v0'::TEXT
      AS expected_accepted_truth_contract,
    'h7_staging_pip_block_population_export_row.v0'::TEXT
      AS output_row_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT
      AS amount_cents_quantization,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days,
    71::NUMBER(38,0) AS expected_accepted_loans,
    2::NUMBER(38,0) AS expected_release_count,
    142::NUMBER(38,0) AS expected_export_rows,
    200::NUMBER(38,0) AS accepted_truth_row_cap,
    200::NUMBER(38,0) AS export_row_cap,
    2000::NUMBER(38,0) AS candidate_bbl_cap_per_release_row
),
sentinel_markers AS (
  SELECT
    ('__BD7BCP_H7_' || 'ACCEPTED_TRUTH_QUERY_ID__')::TEXT
      AS accepted_truth_query_id_unbound_marker
),
release_pins AS (
  SELECT
    column1::TEXT AS mappluto_release,
    TO_DATE(column2::TEXT) AS mappluto_release_dt,
    column3::TEXT AS mappluto_variant,
    'STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES'::TEXT AS mappluto_source_table
  FROM VALUES
    ('26v1', '2026-05-01', 'shoreline_clipped'),
    ('26v2', '2026-08-01', 'shoreline_clipped')
),
release_pin_stats AS (
  SELECT
    COUNT(*) AS release_pin_rows,
    COUNT(DISTINCT mappluto_release || '|' || TO_VARCHAR(mappluto_release_dt)
      || '|' || mappluto_variant) AS distinct_release_pins
  FROM release_pins
),
accepted_scan AS (
  SELECT *
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'))
),
accepted AS (
  SELECT
    row_contract::TEXT AS row_contract,
    bridge_build_id::TEXT AS bridge_build_id,
    TRY_TO_DATE(acris_release_dt::TEXT) AS acris_release_dt,
    property_state::TEXT AS property_state,
    collateral_scope::TEXT AS collateral_scope,
    amount_cents_quantization::TEXT AS amount_cents_quantization,
    round_amount_lattice_cents::NUMBER(38,0) AS round_amount_lattice_cents,
    max_recording_offset_days::NUMBER(38,0) AS max_recording_offset_days,
    eligible_loans::NUMBER(38,0) AS eligible_loans,
    candidate_loans::NUMBER(38,0) AS legal_candidate_loans,
    legal_confirmed_candidate_loans::NUMBER(38,0)
      AS legal_confirmed_candidate_loans,
    accepted_loans::NUMBER(38,0) AS accepted_loans,
    ambiguous_loans::NUMBER(38,0) AS ambiguous_loans,
    candidate_without_legal_loans::NUMBER(38,0)
      AS candidate_without_legal_loans,
    no_candidate_loans::NUMBER(38,0) AS legal_no_candidate_loans,
    selected_multi_parcel_loans::NUMBER(38,0)
      AS selected_multi_parcel_loans,
    whole_export_rows::NUMBER(38,0) AS accepted_truth_export_rows,
    export_row_cap::NUMBER(38,0) AS accepted_truth_export_row_cap,
    export_row_cap_reconciles::BOOLEAN
      AS accepted_truth_export_row_cap_reconciles,
    truth_plane::TEXT AS truth_plane,
    loan_key::TEXT AS loan_key,
    property_keys::NUMBER(38,0) AS property_keys,
    association_plane::TEXT AS association_plane,
    amount_cents::NUMBER(38,0) AS amount_cents,
    TRY_TO_DATE(originationdate::TEXT) AS originationdate,
    originatorname::TEXT AS originatorname,
    originator_match_text::TEXT AS originator_match_text,
    filed_counties AS filed_counties,
    filed_boroughs AS filed_boroughs,
    filed_county_borough_edges AS filed_county_borough_edges,
    distinct_counts AS distinct_counts,
    diagnostic_county_fips AS diagnostic_county_fips,
    bridge_source_record_ids AS bridge_source_record_ids,
    document_id::TEXT AS document_id,
    diagnostic_recorded_borough::NUMBER(38,0)
      AS diagnostic_recorded_borough,
    doc_type::TEXT AS doc_type,
    crfn::TEXT AS crfn,
    TRY_TO_DATE(document_date::TEXT) AS document_date,
    TRY_TO_DATE(recorded_date::TEXT) AS recorded_date,
    recording_offset_days::NUMBER(38,0) AS recording_offset_days,
    lender_match_text::TEXT AS lender_match_text,
    lender_party_type::TEXT AS lender_party_type,
    acris_master_source_record_id::TEXT AS acris_master_source_record_id,
    acris_master_raw_csv_sha256::TEXT AS acris_master_raw_csv_sha256,
    acris_master_filename::TEXT AS acris_master_filename,
    acris_party_source_record_id::TEXT AS acris_party_source_record_id,
    acris_party_raw_csv_sha256::TEXT AS acris_party_raw_csv_sha256,
    acris_party_filename::TEXT AS acris_party_filename,
    truth_bbl_count::NUMBER(38,0) AS truth_bbl_count,
    truth_bbls AS truth_bbls,
    acris_legal_source_records AS acris_legal_source_records
  FROM accepted_scan
),
accepted_stats AS (
  SELECT
    COUNT(*) AS accepted_rows,
    COUNT(DISTINCT loan_key) AS accepted_loans,
    COUNT_IF(COALESCE(row_contract, '')
      <> (SELECT expected_accepted_truth_contract FROM params))
      AS contract_mismatch_rows,
    COUNT_IF(COALESCE(bridge_build_id, '')
      <> (SELECT bridge_build_id FROM params))
      AS bridge_build_mismatch_rows,
    COUNT_IF(COALESCE(collateral_scope, '')
      <> (SELECT collateral_scope FROM params))
      AS collateral_scope_mismatch_rows,
    COUNT_IF(COALESCE(amount_cents_quantization, '')
      <> (SELECT amount_cents_quantization FROM params))
      AS amount_quantization_mismatch_rows,
    COUNT_IF(round_amount_lattice_cents
      <> (SELECT round_amount_lattice_cents FROM params))
      AS round_lattice_mismatch_rows,
    COUNT_IF(max_recording_offset_days
      <> (SELECT max_recording_offset_days FROM params))
      AS offset_window_mismatch_rows,
    COUNT_IF(NOT COALESCE(accepted_truth_export_row_cap_reconciles, FALSE))
      AS upstream_row_cap_failures,
    COUNT_IF(truth_bbl_count IS NULL OR truth_bbl_count < 2)
      AS non_multi_bbl_rows,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS truth_bbl_count_mismatch_rows,
    COUNT_IF(bridge_source_record_ids IS NULL
      OR ARRAY_SIZE(bridge_source_record_ids) = 0)
      AS missing_bridge_source_rows,
    COUNT_IF(acris_legal_source_records IS NULL
      OR ARRAY_SIZE(acris_legal_source_records) < truth_bbl_count)
      AS insufficient_legal_source_rows
  FROM accepted
),
points AS (
  SELECT DISTINCT
    a.loan_key,
    a.truth_plane,
    a.association_plane,
    lip.loan_issuance_property_key::TEXT AS loan_issuance_property_key,
    lip.property_key::TEXT AS property_key,
    lip.latitude::FLOAT AS latitude,
    lip.longitude::FLOAT AS longitude,
    'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:'
      || (SELECT bridge_build_id FROM params)
      || ':'
      || a.loan_key
      || ':'
      || COALESCE(lip.property_key::TEXT, '<null>')
      || ':'
      || COALESCE(lip.loan_issuance_property_key::TEXT, '<null>')
      AS point_source_record_id
  FROM accepted a
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = (SELECT bridge_build_id FROM params)
   AND lip.loan_key = a.loan_key
  WHERE lip.propertystate = 'NY'
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
),
parcels AS (
  SELECT
    p.release AS mappluto_release,
    p.release_dt AS mappluto_release_dt,
    p.variant AS mappluto_variant,
    p.bbl_key,
    SUBSTR(p.bbl_key, 1, 6) AS block_key,
    p.geom_geog,
    p.bbox_xmin,
    p.bbox_ymin,
    p.bbox_xmax,
    p.bbox_ymax,
    p.source_filename,
    p.source_row_number::NUMBER(38,0) AS source_row_number,
    p.geom_wkt_sha256,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:'
      || p.release
      || ':'
      || TO_VARCHAR(p.release_dt)
      || ':'
      || p.variant
      || ':'
      || COALESCE(p.bbl_key, '<null>')
      || ':'
      || COALESCE(TO_VARCHAR(p.source_row_number::NUMBER(38,0)), '<null>')
      AS mappluto_source_record_id
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES p
  JOIN release_pins pin
    ON p.release = pin.mappluto_release
   AND p.release_dt = pin.mappluto_release_dt
   AND p.variant = pin.mappluto_variant
  WHERE p.bbl_key_status = 'valid'
    AND p.key_validation_status = 'valid'
    AND p.bbl_key IS NOT NULL
    AND p.geom_geog IS NOT NULL
),
pip_edges AS (
  SELECT DISTINCT
    pt.loan_key,
    pt.truth_plane,
    pt.association_plane,
    pt.property_key,
    pt.point_source_record_id,
    parcel.mappluto_release,
    parcel.mappluto_release_dt,
    parcel.mappluto_variant,
    parcel.bbl_key AS pip_bbl,
    parcel.block_key,
    parcel.mappluto_source_record_id AS pip_mappluto_source_record_id,
    parcel.source_filename AS pip_source_filename,
    parcel.source_row_number AS pip_source_row_number,
    parcel.geom_wkt_sha256 AS pip_geom_wkt_sha256
  FROM points pt
  JOIN parcels parcel
    ON pt.longitude BETWEEN parcel.bbox_xmin AND parcel.bbox_xmax
   AND pt.latitude BETWEEN parcel.bbox_ymin AND parcel.bbox_ymax
   AND ST_CONTAINS(
     parcel.geom_geog,
     ST_MAKEPOINT(pt.longitude, pt.latitude)
   )
),
pip_blocks AS (
  SELECT DISTINCT
    loan_key,
    truth_plane,
    association_plane,
    property_key,
    point_source_record_id,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant,
    block_key
  FROM pip_edges
),
candidate_edges AS (
  SELECT DISTINCT
    b.loan_key,
    b.truth_plane,
    b.association_plane,
    b.mappluto_release,
    b.mappluto_release_dt,
    b.mappluto_variant,
    b.block_key,
    p.bbl_key AS candidate_bbl,
    p.mappluto_source_record_id,
    p.source_filename,
    p.source_row_number,
    p.geom_wkt_sha256
  FROM pip_blocks b
  JOIN parcels p
    ON p.mappluto_release = b.mappluto_release
   AND p.mappluto_release_dt = b.mappluto_release_dt
   AND p.mappluto_variant = b.mappluto_variant
   AND p.block_key = b.block_key
),
point_source_records AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    ARRAY_AGG(DISTINCT point_source_record_id)
      WITHIN GROUP (ORDER BY point_source_record_id)
      AS point_source_record_ids,
    COUNT(DISTINCT property_key) AS property_point_rows
  FROM (
    SELECT DISTINCT
      loan_key,
      truth_plane,
      association_plane,
      point_source_record_id,
      property_key
    FROM points
  )
  GROUP BY loan_key, truth_plane, association_plane
),
pip_counts AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant,
    COUNT(DISTINCT property_key) AS pip_reached_points,
    COUNT(DISTINCT block_key) AS pip_block_count,
    COUNT(DISTINCT pip_bbl) AS pip_bbl_count
  FROM pip_edges
  GROUP BY
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant
),
candidate_bbl_sets AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant,
    ARRAY_AGG(DISTINCT candidate_bbl)
      WITHIN GROUP (ORDER BY candidate_bbl) AS candidate_bbls,
    COUNT(DISTINCT candidate_bbl) AS candidate_bbl_count
  FROM candidate_edges
  GROUP BY
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant
),
candidate_source_arrays AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant,
    ARRAY_AGG(DISTINCT mappluto_source_record_id)
      WITHIN GROUP (ORDER BY mappluto_source_record_id)
      AS candidate_source_record_ids,
    ARRAY_AGG(DISTINCT geom_wkt_sha256)
      WITHIN GROUP (ORDER BY geom_wkt_sha256)
      AS candidate_geom_wkt_sha256s,
    COUNT(DISTINCT mappluto_source_record_id) AS candidate_source_record_count
  FROM candidate_edges
  GROUP BY
    loan_key,
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant
),
subject_releases AS (
  SELECT
    a.*,
    pin.mappluto_release,
    pin.mappluto_release_dt,
    pin.mappluto_variant,
    pin.mappluto_source_table
  FROM accepted a
  CROSS JOIN release_pins pin
),
truth_edges AS (
  SELECT
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    s.mappluto_release,
    s.mappluto_release_dt,
    s.mappluto_variant,
    truth.value::TEXT AS truth_bbl
  FROM subject_releases s,
    LATERAL FLATTEN(input => s.truth_bbls) truth
),
truth_hits AS (
  SELECT
    t.loan_key,
    t.truth_plane,
    t.association_plane,
    t.mappluto_release,
    t.mappluto_release_dt,
    t.mappluto_variant,
    COUNT(DISTINCT t.truth_bbl) AS truth_bbls,
    COUNT(DISTINCT IFF(c.candidate_bbl IS NOT NULL, t.truth_bbl, NULL))
      AS reached_truth_bbls
  FROM truth_edges t
  LEFT JOIN candidate_edges c
    ON c.loan_key = t.loan_key
   AND c.truth_plane = t.truth_plane
   AND c.association_plane = t.association_plane
   AND c.mappluto_release = t.mappluto_release
   AND c.mappluto_release_dt = t.mappluto_release_dt
   AND c.mappluto_variant = t.mappluto_variant
   AND c.candidate_bbl = t.truth_bbl
  GROUP BY
    t.loan_key,
    t.truth_plane,
    t.association_plane,
    t.mappluto_release,
    t.mappluto_release_dt,
    t.mappluto_variant
),
export_rows AS (
  SELECT
    s.*,
    COALESCE(ps.property_point_rows, 0) AS property_point_rows,
    COALESCE(pc.pip_reached_points, 0) AS pip_reached_points,
    COALESCE(pc.pip_block_count, 0) AS pip_block_count,
    COALESCE(pc.pip_bbl_count, 0) AS pip_bbl_count,
    COALESCE(cb.candidate_bbl_count, 0) AS candidate_bbl_count,
    COALESCE(ca.candidate_source_record_count, 0)
      AS candidate_source_record_count,
    COALESCE(h.reached_truth_bbls, 0) AS reached_truth_bbls,
    CASE
      WHEN COALESCE(h.reached_truth_bbls, 0) = s.truth_bbl_count THEN 'full'
      WHEN COALESCE(h.reached_truth_bbls, 0) = 0 THEN 'none'
      ELSE 'partial'
    END AS reach_status,
    COALESCE(ps.point_source_record_ids, ARRAY_CONSTRUCT())
      AS point_source_record_ids,
    COALESCE(cb.candidate_bbls, ARRAY_CONSTRUCT()) AS candidate_bbls,
    COALESCE(ca.candidate_source_record_ids, ARRAY_CONSTRUCT())
      AS candidate_source_record_ids,
    COALESCE(ca.candidate_geom_wkt_sha256s, ARRAY_CONSTRUCT())
      AS candidate_geom_wkt_sha256s
  FROM subject_releases s
  LEFT JOIN point_source_records ps
    ON ps.loan_key = s.loan_key
   AND ps.truth_plane = s.truth_plane
   AND ps.association_plane = s.association_plane
  LEFT JOIN pip_counts pc
    ON pc.loan_key = s.loan_key
   AND pc.truth_plane = s.truth_plane
   AND pc.association_plane = s.association_plane
   AND pc.mappluto_release = s.mappluto_release
   AND pc.mappluto_release_dt = s.mappluto_release_dt
   AND pc.mappluto_variant = s.mappluto_variant
  LEFT JOIN candidate_bbl_sets cb
    ON cb.loan_key = s.loan_key
   AND cb.truth_plane = s.truth_plane
   AND cb.association_plane = s.association_plane
   AND cb.mappluto_release = s.mappluto_release
   AND cb.mappluto_release_dt = s.mappluto_release_dt
   AND cb.mappluto_variant = s.mappluto_variant
  LEFT JOIN candidate_source_arrays ca
    ON ca.loan_key = s.loan_key
   AND ca.truth_plane = s.truth_plane
   AND ca.association_plane = s.association_plane
   AND ca.mappluto_release = s.mappluto_release
   AND ca.mappluto_release_dt = s.mappluto_release_dt
   AND ca.mappluto_variant = s.mappluto_variant
  LEFT JOIN truth_hits h
    ON h.loan_key = s.loan_key
   AND h.truth_plane = s.truth_plane
   AND h.association_plane = s.association_plane
   AND h.mappluto_release = s.mappluto_release
   AND h.mappluto_release_dt = s.mappluto_release_dt
   AND h.mappluto_variant = s.mappluto_variant
),
export_stats AS (
  SELECT
    COUNT(*) AS export_rows,
    COUNT(DISTINCT loan_key) AS export_accepted_loans,
    COUNT(DISTINCT loan_key || '|' || mappluto_release || '|'
      || TO_VARCHAR(mappluto_release_dt) || '|' || mappluto_variant)
      AS unique_subject_release_rows,
    COUNT_IF(candidate_bbl_count = 0) AS zero_candidate_release_rows,
    COUNT_IF(candidate_bbl_count <> ARRAY_SIZE(candidate_bbls))
      AS candidate_bbl_count_mismatch_rows,
    COUNT_IF(candidate_source_record_count <> ARRAY_SIZE(candidate_source_record_ids))
      AS candidate_source_count_mismatch_rows,
    COUNT_IF(candidate_bbl_count <> candidate_source_record_count)
      AS candidate_source_bbl_mismatch_rows,
    COUNT_IF(candidate_bbl_count = 0
      AND ARRAY_SIZE(candidate_geom_wkt_sha256s) <> 0)
      AS zero_candidate_digest_leakage_rows,
    COUNT_IF(candidate_bbl_count > (SELECT candidate_bbl_cap_per_release_row
                                   FROM params))
      AS candidate_bbl_cap_failures,
    COUNT_IF(reached_truth_bbls > truth_bbl_count)
      AS reached_truth_accounting_failures,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS final_truth_count_mismatch_rows,
    COUNT_IF(reach_status NOT IN ('full', 'partial', 'none'))
      AS invalid_reach_status_rows,
    COUNT_IF(candidate_bbl_count = 0 AND reach_status <> 'none')
      AS zero_candidate_reach_mismatch_rows,
    COUNT_IF(mappluto_release NOT IN ('26v1', '26v2'))
      AS invalid_release_rows,
    COUNT_IF(mappluto_variant <> 'shoreline_clipped')
      AS invalid_variant_rows,
    COUNT_IF(mappluto_release = '26v1'
      AND mappluto_release_dt <> '2026-05-01'::DATE)
      AS invalid_26v1_release_dt_rows,
    COUNT_IF(mappluto_release = '26v2'
      AND mappluto_release_dt <> '2026-08-01'::DATE)
      AS invalid_26v2_release_dt_rows
  FROM export_rows
),
reach_denominators AS (
  SELECT
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant,
    COUNT(*) AS release_rows_in_stratum,
    COUNT_IF(reach_status = 'full') AS full_reach_subjects,
    COUNT_IF(reach_status = 'partial') AS partial_reach_subjects,
    COUNT_IF(reach_status = 'none') AS no_reach_subjects,
    COUNT_IF(candidate_bbl_count = 0) AS zero_candidate_subjects,
    SUM(truth_bbl_count) AS truth_bbl_edges,
    SUM(reached_truth_bbls) AS reached_truth_bbl_edges,
    MIN(candidate_bbl_count) AS min_candidate_bbls,
    MEDIAN(candidate_bbl_count) AS median_candidate_bbls,
    PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY candidate_bbl_count)
      AS p90_candidate_bbls,
    MAX(candidate_bbl_count) AS max_candidate_bbls,
    SUM(candidate_bbl_count) AS candidate_bbl_edges,
    COUNT_IF(reached_truth_bbls > truth_bbl_count)
      AS reach_accounting_failures
  FROM export_rows
  GROUP BY
    truth_plane,
    association_plane,
    mappluto_release,
    mappluto_release_dt,
    mappluto_variant
),
whole_denominator AS (
  SELECT
    COUNT(*) AS release_rows,
    COUNT(DISTINCT loan_key) AS accepted_loans,
    COUNT_IF(reach_status = 'full') AS full_reach_release_rows,
    COUNT_IF(reach_status = 'partial') AS partial_reach_release_rows,
    COUNT_IF(reach_status = 'none') AS no_reach_release_rows,
    COUNT_IF(candidate_bbl_count = 0) AS zero_candidate_release_rows,
    SUM(truth_bbl_count) AS truth_bbl_edges,
    SUM(reached_truth_bbls) AS reached_truth_bbl_edges,
    SUM(candidate_bbl_count) AS candidate_bbl_edges
  FROM export_rows
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT
      'accepted_truth_query_id_sentinel_unsubstituted' AS failure_reason,
      (SELECT accepted_truth_query_id FROM params)
        = (SELECT accepted_truth_query_id_unbound_marker
           FROM sentinel_markers) AS failed
    UNION ALL
    SELECT 'release_pin_count_mismatch',
      (SELECT release_pin_rows FROM release_pin_stats)
        <> (SELECT expected_release_count FROM params)
    UNION ALL
    SELECT 'duplicate_release_pin',
      (SELECT release_pin_rows FROM release_pin_stats)
        <> (SELECT distinct_release_pins FROM release_pin_stats)
    UNION ALL
    SELECT 'accepted_truth_result_empty',
      (SELECT accepted_rows FROM accepted_stats) = 0
    UNION ALL
    SELECT 'accepted_truth_result_exceeds_bound',
      (SELECT accepted_rows FROM accepted_stats)
        > (SELECT accepted_truth_row_cap FROM params)
    UNION ALL
    SELECT 'accepted_truth_result_not_expected_71',
      (SELECT accepted_rows FROM accepted_stats)
        <> (SELECT expected_accepted_loans FROM params)
    UNION ALL
    SELECT 'accepted_truth_repeats_loan',
      (SELECT accepted_rows FROM accepted_stats)
        <> (SELECT accepted_loans FROM accepted_stats)
    UNION ALL
    SELECT 'accepted_truth_contract_mismatch',
      (SELECT contract_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_bridge_build_mismatch',
      (SELECT bridge_build_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_collateral_scope_mismatch',
      (SELECT collateral_scope_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_amount_quantization_mismatch',
      (SELECT amount_quantization_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_round_lattice_mismatch',
      (SELECT round_lattice_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_offset_window_mismatch',
      (SELECT offset_window_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_upstream_row_cap_failure',
      (SELECT upstream_row_cap_failures FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_non_multi_bbl_row',
      (SELECT non_multi_bbl_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_bbl_count_mismatch',
      (SELECT truth_bbl_count_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_missing_bridge_source_records',
      (SELECT missing_bridge_source_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_insufficient_legal_source_records',
      (SELECT insufficient_legal_source_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'export_row_count_mismatch',
      (SELECT export_rows FROM export_stats)
        <> (SELECT expected_export_rows FROM params)
    UNION ALL
    SELECT 'export_row_count_exceeds_bound',
      (SELECT export_rows FROM export_stats)
        > (SELECT export_row_cap FROM params)
    UNION ALL
    SELECT 'export_duplicate_subject_release',
      (SELECT export_rows FROM export_stats)
        <> (SELECT unique_subject_release_rows FROM export_stats)
    UNION ALL
    SELECT 'export_accepted_loan_count_mismatch',
      (SELECT export_accepted_loans FROM export_stats)
        <> (SELECT expected_accepted_loans FROM params)
    UNION ALL
    SELECT 'candidate_bbl_count_mismatch',
      (SELECT candidate_bbl_count_mismatch_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'candidate_source_count_mismatch',
      (SELECT candidate_source_count_mismatch_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'candidate_source_bbl_mismatch',
      (SELECT candidate_source_bbl_mismatch_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'zero_candidate_digest_leakage',
      (SELECT zero_candidate_digest_leakage_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'candidate_bbl_cap_exceeded',
      (SELECT candidate_bbl_cap_failures FROM export_stats) <> 0
    UNION ALL
    SELECT 'reached_truth_accounting_failure',
      (SELECT reached_truth_accounting_failures FROM export_stats) <> 0
    UNION ALL
    SELECT 'final_truth_count_mismatch',
      (SELECT final_truth_count_mismatch_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'invalid_reach_status',
      (SELECT invalid_reach_status_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'zero_candidate_reach_mismatch',
      (SELECT zero_candidate_reach_mismatch_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'release_pin_mismatch',
      (SELECT invalid_release_rows FROM export_stats) <> 0
        OR (SELECT invalid_variant_rows FROM export_stats) <> 0
        OR (SELECT invalid_26v1_release_dt_rows FROM export_stats) <> 0
        OR (SELECT invalid_26v2_release_dt_rows FROM export_stats) <> 0
    UNION ALL
    SELECT 'reach_denominator_accounting_failure',
      EXISTS (
        SELECT 1
        FROM reach_denominators
        WHERE release_rows_in_stratum
          <> full_reach_subjects + partial_reach_subjects + no_reach_subjects
           OR reach_accounting_failures <> 0
      )
    UNION ALL
    SELECT 'whole_reach_denominator_accounting_failure',
      (SELECT release_rows FROM whole_denominator)
        <> (SELECT full_reach_release_rows + partial_reach_release_rows
            + no_reach_release_rows FROM whole_denominator)
        OR (SELECT reached_truth_bbl_edges FROM whole_denominator)
          > (SELECT truth_bbl_edges FROM whole_denominator)
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    MIN(failure_reason) AS refusal_reason
  FROM guard_failures
),
guard_output AS (
  SELECT
    (SELECT output_row_contract FROM params) AS row_contract,
    'guard_failure'::TEXT AS row_kind,
    g.guard_status,
    g.refusal_reason,
    (SELECT accepted_truth_query_id FROM params) AS accepted_truth_query_id,
    NULL::TEXT AS loan_key,
    NULL::TEXT AS truth_plane,
    NULL::TEXT AS association_plane,
    NULL::TEXT AS mappluto_release,
    NULL::DATE AS mappluto_release_dt,
    NULL::TEXT AS mappluto_variant,
    (SELECT bridge_build_id FROM params) AS bridge_build_id,
    NULL::DATE AS acris_release_dt,
    (SELECT collateral_scope FROM params) AS collateral_scope,
    NULL::NUMBER(38,0) AS accepted_plane_eligible_loans,
    NULL::NUMBER(38,0) AS accepted_plane_legal_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_legal_confirmed_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_accepted_loans,
    NULL::NUMBER(38,0) AS accepted_plane_ambiguous_loans,
    NULL::NUMBER(38,0) AS accepted_plane_candidate_without_legal_loans,
    NULL::NUMBER(38,0) AS accepted_plane_no_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_selected_multi_parcel_loans,
    NULL::NUMBER(38,0) AS whole_accepted_loans,
    NULL::NUMBER(38,0) AS whole_release_rows,
    NULL::NUMBER(38,0) AS whole_full_reach_release_rows,
    NULL::NUMBER(38,0) AS whole_partial_reach_release_rows,
    NULL::NUMBER(38,0) AS whole_no_reach_release_rows,
    NULL::NUMBER(38,0) AS whole_zero_candidate_release_rows,
    NULL::NUMBER(38,0) AS stratum_release_rows,
    NULL::NUMBER(38,0) AS stratum_full_reach_subjects,
    NULL::NUMBER(38,0) AS stratum_partial_reach_subjects,
    NULL::NUMBER(38,0) AS stratum_no_reach_subjects,
    NULL::NUMBER(38,0) AS stratum_zero_candidate_subjects,
    NULL::NUMBER(38,0) AS stratum_truth_bbl_edges,
    NULL::NUMBER(38,0) AS stratum_reached_truth_bbl_edges,
    NULL::NUMBER(38,0) AS stratum_candidate_bbl_edges,
    NULL::NUMBER(38,0) AS stratum_min_candidate_bbls,
    NULL::FLOAT AS stratum_median_candidate_bbls,
    NULL::FLOAT AS stratum_p90_candidate_bbls,
    NULL::NUMBER(38,0) AS stratum_max_candidate_bbls,
    NULL::NUMBER(38,0) AS property_keys,
    NULL::NUMBER(38,0) AS property_point_rows,
    NULL::NUMBER(38,0) AS pip_reached_points,
    NULL::NUMBER(38,0) AS pip_block_count,
    NULL::NUMBER(38,0) AS pip_bbl_count,
    NULL::NUMBER(38,0) AS candidate_bbl_count,
    NULL::NUMBER(38,0) AS candidate_source_record_count,
    NULL::NUMBER(38,0) AS truth_bbl_count,
    NULL::NUMBER(38,0) AS reached_truth_bbls,
    NULL::TEXT AS reach_status,
    NULL::NUMBER(38,0) AS amount_cents,
    NULL::DATE AS originationdate,
    NULL::TEXT AS originatorname,
    NULL::TEXT AS originator_match_text,
    NULL::VARIANT AS filed_counties,
    NULL::VARIANT AS filed_boroughs,
    NULL::VARIANT AS filed_county_borough_edges,
    NULL::VARIANT AS distinct_counts,
    NULL::VARIANT AS diagnostic_county_fips,
    NULL::VARIANT AS bridge_source_record_ids,
    NULL::TEXT AS document_id,
    NULL::NUMBER(38,0) AS diagnostic_recorded_borough,
    NULL::TEXT AS doc_type,
    NULL::TEXT AS crfn,
    NULL::DATE AS document_date,
    NULL::DATE AS recorded_date,
    NULL::NUMBER(38,0) AS recording_offset_days,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::TEXT AS acris_master_source_record_id,
    NULL::TEXT AS acris_master_raw_csv_sha256,
    NULL::TEXT AS acris_master_filename,
    NULL::TEXT AS acris_party_source_record_id,
    NULL::TEXT AS acris_party_raw_csv_sha256,
    NULL::TEXT AS acris_party_filename,
    NULL::VARIANT AS truth_bbls,
    NULL::VARIANT AS acris_legal_source_records,
    NULL::VARIANT AS point_source_record_ids,
    NULL::VARIANT AS candidate_bbls,
    NULL::VARIANT AS candidate_source_record_ids,
    NULL::VARIANT AS candidate_geom_wkt_sha256s
  FROM guard_failures f
  CROSS JOIN guard_summary g
),
accepted_output AS (
  SELECT
    (SELECT output_row_contract FROM params) AS row_contract,
    'accepted_release_candidate_set'::TEXT AS row_kind,
    g.guard_status,
    g.refusal_reason,
    (SELECT accepted_truth_query_id FROM params) AS accepted_truth_query_id,
    r.loan_key,
    r.truth_plane,
    r.association_plane,
    r.mappluto_release,
    r.mappluto_release_dt,
    r.mappluto_variant,
    r.bridge_build_id,
    r.acris_release_dt,
    r.collateral_scope,
    r.eligible_loans AS accepted_plane_eligible_loans,
    r.legal_candidate_loans AS accepted_plane_legal_candidate_loans,
    r.legal_confirmed_candidate_loans
      AS accepted_plane_legal_confirmed_candidate_loans,
    r.accepted_loans AS accepted_plane_accepted_loans,
    r.ambiguous_loans AS accepted_plane_ambiguous_loans,
    r.candidate_without_legal_loans
      AS accepted_plane_candidate_without_legal_loans,
    r.legal_no_candidate_loans AS accepted_plane_no_candidate_loans,
    r.selected_multi_parcel_loans
      AS accepted_plane_selected_multi_parcel_loans,
    w.accepted_loans AS whole_accepted_loans,
    w.release_rows AS whole_release_rows,
    w.full_reach_release_rows AS whole_full_reach_release_rows,
    w.partial_reach_release_rows AS whole_partial_reach_release_rows,
    w.no_reach_release_rows AS whole_no_reach_release_rows,
    w.zero_candidate_release_rows AS whole_zero_candidate_release_rows,
    d.release_rows_in_stratum AS stratum_release_rows,
    d.full_reach_subjects AS stratum_full_reach_subjects,
    d.partial_reach_subjects AS stratum_partial_reach_subjects,
    d.no_reach_subjects AS stratum_no_reach_subjects,
    d.zero_candidate_subjects AS stratum_zero_candidate_subjects,
    d.truth_bbl_edges AS stratum_truth_bbl_edges,
    d.reached_truth_bbl_edges AS stratum_reached_truth_bbl_edges,
    d.candidate_bbl_edges AS stratum_candidate_bbl_edges,
    d.min_candidate_bbls AS stratum_min_candidate_bbls,
    d.median_candidate_bbls AS stratum_median_candidate_bbls,
    d.p90_candidate_bbls AS stratum_p90_candidate_bbls,
    d.max_candidate_bbls AS stratum_max_candidate_bbls,
    r.property_keys,
    r.property_point_rows,
    r.pip_reached_points,
    r.pip_block_count,
    r.pip_bbl_count,
    r.candidate_bbl_count,
    r.candidate_source_record_count,
    r.truth_bbl_count,
    r.reached_truth_bbls,
    r.reach_status,
    r.amount_cents,
    r.originationdate,
    r.originatorname,
    r.originator_match_text,
    r.filed_counties,
    r.filed_boroughs,
    r.filed_county_borough_edges,
    r.distinct_counts,
    r.diagnostic_county_fips,
    r.bridge_source_record_ids,
    r.document_id,
    r.diagnostic_recorded_borough,
    r.doc_type,
    r.crfn,
    r.document_date,
    r.recorded_date,
    r.recording_offset_days,
    r.lender_match_text,
    r.lender_party_type,
    r.acris_master_source_record_id,
    r.acris_master_raw_csv_sha256,
    r.acris_master_filename,
    r.acris_party_source_record_id,
    r.acris_party_raw_csv_sha256,
    r.acris_party_filename,
    r.truth_bbls,
    r.acris_legal_source_records,
    r.point_source_record_ids,
    r.candidate_bbls,
    r.candidate_source_record_ids,
    r.candidate_geom_wkt_sha256s
  FROM export_rows r
  JOIN reach_denominators d
    ON d.truth_plane = r.truth_plane
   AND d.association_plane = r.association_plane
   AND d.mappluto_release = r.mappluto_release
   AND d.mappluto_release_dt = r.mappluto_release_dt
   AND d.mappluto_variant = r.mappluto_variant
  CROSS JOIN whole_denominator w
  CROSS JOIN guard_summary g
  WHERE g.guard_status = 'ok'
)
SELECT *
FROM accepted_output
UNION ALL
SELECT *
FROM guard_output
ORDER BY row_kind, truth_plane, loan_key, mappluto_release;
