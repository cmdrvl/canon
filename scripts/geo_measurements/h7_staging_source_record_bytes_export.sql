-- Appendix H.7 source-record byte payload export over PIP-block candidate rows.
--
-- This second-stage staging export consumes one successful
-- h7_staging_pip_block_population_export_row.v0 RESULT_SCAN and emits exactly
-- one row per accepted H7 loan and MapPLUTO release. It preserves the first
-- stage's PIP-block candidate relation and adds source evidence wrappers for
-- the committed input-only source_record_bytes_base64 path.
--
-- The source_record_bytes_base64 payload is strict-padded standard BASE64 over
-- a UTF-8 canonical string:
--
--   TO_JSON(ARRAY_CONSTRUCT(
--     ARRAY_CONSTRUCT('payload_contract', 'h7_derived_source_record_payload.v0'),
--     ARRAY_CONSTRUCT('source_record_class', 'derived_immutable_evidence_record'),
--     ...
--   ))
--
-- The payload is a positionally fixed array of key/value pairs. It contains
-- actual evidence values plus locators and upstream hashes when available. It
-- is derived immutable evidence for Canon replay; it is not original source
-- bytes and does not make source-record count evidence weight.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_PIP_BLOCK_POPULATION_QUERY_ID__'
--
-- Positive input control:
-- * 01c6c174-0821-aa0e-006c-c703088dc742 produced 142 successful
--   h7_staging_pip_block_population_export_row.v0 release rows from 71
--   accepted H7 subjects.

WITH
params AS (
  SELECT
    '__BD7BCP_H7_PIP_BLOCK_POPULATION_QUERY_ID__'::TEXT
      AS pip_block_population_query_id,
    'h7_staging_pip_block_population_export_row.v0'::TEXT
      AS expected_input_row_contract,
    'h7_staging_source_record_bytes_export_row.v0'::TEXT
      AS output_row_contract,
    'h7_derived_source_record_payload.v0'::TEXT AS payload_contract,
    'derived_immutable_evidence_record'::TEXT AS source_record_class,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS expected_acris_release_dt,
    'NY'::TEXT AS expected_property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT
      AS amount_cents_quantization,
    'nyc_dcp_mappluto_geometry_evidence.v3'::TEXT
      AS mappluto_geometry_contract_version,
    71::NUMBER(38,0) AS expected_accepted_loans,
    2::NUMBER(38,0) AS expected_release_count,
    142::NUMBER(38,0) AS expected_release_rows,
    200::NUMBER(38,0) AS input_row_cap,
    200::NUMBER(38,0) AS output_row_cap,
    2000::NUMBER(38,0) AS candidate_bbl_cap_per_release_row,
    1200::NUMBER(38,0) AS source_record_cap_per_release_row,
    4096::NUMBER(38,0) AS source_record_payload_utf8_cap,
    2000000::NUMBER(38,0) AS row_payload_utf8_cap
),
sentinel_markers AS (
  SELECT
    ('__BD7BCP_H7_' || 'PIP_BLOCK_POPULATION_QUERY_ID__')::TEXT
      AS pip_block_population_query_id_unbound_marker
),
population_scan AS (
  SELECT *
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_PIP_BLOCK_POPULATION_QUERY_ID__'))
),
population_rows AS (
  SELECT
    row_contract::TEXT AS row_contract,
    row_kind::TEXT AS row_kind,
    guard_status::TEXT AS upstream_guard_status,
    refusal_reason::TEXT AS upstream_refusal_reason,
    accepted_truth_query_id::TEXT AS accepted_truth_query_id,
    loan_key::TEXT AS loan_key,
    truth_plane::TEXT AS truth_plane,
    association_plane::TEXT AS association_plane,
    mappluto_release::TEXT AS mappluto_release,
    TRY_TO_DATE(mappluto_release_dt::TEXT) AS mappluto_release_dt,
    mappluto_variant::TEXT AS mappluto_variant,
    bridge_build_id::TEXT AS bridge_build_id,
    TRY_TO_DATE(acris_release_dt::TEXT) AS acris_release_dt,
    collateral_scope::TEXT AS collateral_scope,
    accepted_plane_eligible_loans::NUMBER(38,0)
      AS accepted_plane_eligible_loans,
    accepted_plane_legal_candidate_loans::NUMBER(38,0)
      AS accepted_plane_legal_candidate_loans,
    accepted_plane_legal_confirmed_candidate_loans::NUMBER(38,0)
      AS accepted_plane_legal_confirmed_candidate_loans,
    accepted_plane_accepted_loans::NUMBER(38,0)
      AS accepted_plane_accepted_loans,
    accepted_plane_ambiguous_loans::NUMBER(38,0)
      AS accepted_plane_ambiguous_loans,
    accepted_plane_candidate_without_legal_loans::NUMBER(38,0)
      AS accepted_plane_candidate_without_legal_loans,
    accepted_plane_no_candidate_loans::NUMBER(38,0)
      AS accepted_plane_no_candidate_loans,
    accepted_plane_selected_multi_parcel_loans::NUMBER(38,0)
      AS accepted_plane_selected_multi_parcel_loans,
    whole_accepted_loans::NUMBER(38,0) AS whole_accepted_loans,
    whole_release_rows::NUMBER(38,0) AS whole_release_rows,
    whole_zero_candidate_release_rows::NUMBER(38,0)
      AS whole_zero_candidate_release_rows,
    property_keys::NUMBER(38,0) AS property_keys,
    property_point_rows::NUMBER(38,0) AS property_point_rows,
    pip_reached_points::NUMBER(38,0) AS pip_reached_points,
    pip_block_count::NUMBER(38,0) AS pip_block_count,
    pip_bbl_count::NUMBER(38,0) AS pip_bbl_count,
    candidate_bbl_count::NUMBER(38,0) AS candidate_bbl_count,
    candidate_source_record_count::NUMBER(38,0)
      AS candidate_source_record_count,
    truth_bbl_count::NUMBER(38,0) AS truth_bbl_count,
    reached_truth_bbls::NUMBER(38,0) AS reached_truth_bbls,
    reach_status::TEXT AS reach_status,
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
    truth_bbls AS truth_bbls,
    acris_legal_source_records AS acris_legal_source_records,
    point_source_record_ids AS point_source_record_ids,
    candidate_bbls AS candidate_bbls,
    candidate_source_record_ids AS candidate_source_record_ids,
    candidate_geom_wkt_sha256s AS candidate_geom_wkt_sha256s
  FROM population_scan
),
accepted AS (
  SELECT
    *,
    loan_key || '|' || truth_plane || '|' || association_plane || '|'
      || mappluto_release || '|' || TO_VARCHAR(mappluto_release_dt) || '|'
      || mappluto_variant AS row_key
  FROM population_rows
  WHERE row_kind = 'accepted_release_candidate_set'
    AND upstream_guard_status = 'ok'
),
input_stats AS (
  SELECT
    COUNT(*) AS input_release_rows,
    COUNT(DISTINCT loan_key) AS accepted_loans,
    COUNT(DISTINCT row_key) AS distinct_release_rows,
    COUNT(DISTINCT mappluto_release || '|'
      || TO_VARCHAR(mappluto_release_dt) || '|' || mappluto_variant)
      AS distinct_release_pins,
    COUNT_IF(row_contract <> (SELECT expected_input_row_contract FROM params))
      AS input_contract_mismatch_rows,
    COUNT_IF(bridge_build_id <> (SELECT bridge_build_id FROM params))
      AS bridge_build_mismatch_rows,
    COUNT_IF(collateral_scope <> (SELECT collateral_scope FROM params))
      AS collateral_scope_mismatch_rows,
    COUNT_IF(acris_release_dt IS NULL
      OR acris_release_dt <> (SELECT expected_acris_release_dt FROM params))
      AS acris_release_dt_mismatch_rows,
    COUNT_IF(candidate_bbl_count <> ARRAY_SIZE(candidate_bbls))
      AS candidate_bbl_count_mismatch_rows,
    COUNT_IF(candidate_source_record_count <> ARRAY_SIZE(candidate_source_record_ids))
      AS candidate_source_count_mismatch_rows,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS truth_bbl_count_mismatch_rows,
    COUNT_IF(TYPEOF(candidate_bbls) <> 'ARRAY'
      OR TYPEOF(truth_bbls) <> 'ARRAY'
      OR TYPEOF(acris_legal_source_records) <> 'ARRAY'
      OR TYPEOF(point_source_record_ids) <> 'ARRAY')
      AS array_mapping_failures,
    COUNT_IF(candidate_bbl_count = 0 AND ARRAY_SIZE(candidate_bbls) <> 0)
      AS zero_candidate_array_leakage_rows,
    COUNT_IF(candidate_bbl_count > (SELECT candidate_bbl_cap_per_release_row
                                   FROM params))
      AS candidate_cap_failures,
    COUNT_IF(reached_truth_bbls > truth_bbl_count)
      AS reached_truth_accounting_failures,
    COUNT_IF(mappluto_release NOT IN ('26v1', '26v2')
      OR mappluto_variant <> 'shoreline_clipped'
      OR (mappluto_release = '26v1'
        AND mappluto_release_dt <> '2026-05-01'::DATE)
      OR (mappluto_release = '26v2'
        AND mappluto_release_dt <> '2026-08-01'::DATE))
      AS release_pin_failures,
    COUNT_IF(truth_plane = 'round_exact_lender_party'
      AND (acris_party_source_record_id IS NULL
        OR acris_party_raw_csv_sha256 IS NULL
        OR acris_party_filename IS NULL
        OR lender_match_text IS NULL
        OR lender_party_type IS NULL))
      AS missing_round_party_rows
  FROM accepted
),
point_source_ids AS (
  SELECT DISTINCT
    a.row_key,
    point.value::TEXT AS point_source_record_id
  FROM accepted a,
    LATERAL FLATTEN(input => a.point_source_record_ids) point
),
bridge_records AS (
  SELECT DISTINCT
    a.row_key,
    10::NUMBER(9,0) AS role_order,
    'bridge_loan'::TEXT AS role,
    ARRAY_CONSTRUCT() AS parcel_ids,
    NULL::TEXT AS parcel_id,
    p.point_source_record_id AS source_record_id,
    a.bridge_build_id::TEXT AS source_vintage,
    NULL::TEXT AS source_filename,
    NULL::NUMBER(38,0) AS source_row_number,
    NULL::TEXT AS upstream_sha256,
    lip.propertystate::TEXT AS bridge_property_state,
    TO_JSON(ARRAY_CONSTRUCT(
      ARRAY_CONSTRUCT('payload_contract',
        (SELECT payload_contract FROM params)),
      ARRAY_CONSTRUCT('source_record_class',
        (SELECT source_record_class FROM params)),
      ARRAY_CONSTRUCT('role', 'bridge_loan'),
      ARRAY_CONSTRUCT('source_table',
        'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY'),
      ARRAY_CONSTRUCT('source_record_id', p.point_source_record_id),
      ARRAY_CONSTRUCT('source_vintage', a.bridge_build_id::TEXT),
      ARRAY_CONSTRUCT('build_id', lip.build_id::TEXT),
      ARRAY_CONSTRUCT('loan_key', lip.loan_key::TEXT),
      ARRAY_CONSTRUCT('property_key', lip.property_key::TEXT),
      ARRAY_CONSTRUCT('loan_issuance_property_key',
        lip.loan_issuance_property_key::TEXT),
      ARRAY_CONSTRUCT('source_loan_observation_key',
        COALESCE(lip.source_loan_observation_key::TEXT, '<null>')),
      ARRAY_CONSTRUCT('cik', COALESCE(lip.cik::TEXT, '<null>')),
      ARRAY_CONSTRUCT('assetnumber',
        COALESCE(lip.assetnumber::TEXT, '<null>')),
      ARRAY_CONSTRUCT('loan_asset_key',
        COALESCE(lip.loan_asset_key::TEXT, '<null>')),
      ARRAY_CONSTRUCT('property_ordinal',
        COALESCE(lip.property_ordinal::TEXT, '<null>')),
      ARRAY_CONSTRUCT('loannumber',
        COALESCE(lip.loannumber::TEXT, '<null>')),
      ARRAY_CONSTRUCT('originatorname',
        COALESCE(lip.originatorname::TEXT, '<null>')),
      ARRAY_CONSTRUCT('originator_match_text',
        COALESCE(lip.originator_match_text::TEXT, '<null>')),
      ARRAY_CONSTRUCT('originationdate',
        COALESCE(TO_VARCHAR(lip.originationdate, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('originalloanamount',
        COALESCE(TO_VARCHAR(lip.originalloanamount), '<null>')),
      ARRAY_CONSTRUCT('amount_cents_quantization',
        (SELECT amount_cents_quantization FROM params)),
      ARRAY_CONSTRUCT('amount_cents', TO_VARCHAR(a.amount_cents)),
      ARRAY_CONSTRUCT('propertystate',
        COALESCE(lip.propertystate::TEXT, '<null>')),
      ARRAY_CONSTRUCT('propertycounty',
        COALESCE(lip.propertycounty::TEXT, '<null>')),
      ARRAY_CONSTRUCT('county_fips',
        COALESCE(lip.county_fips::TEXT, '<null>')),
      ARRAY_CONSTRUCT('latitude', COALESCE(TO_VARCHAR(lip.latitude), '<null>')),
      ARRAY_CONSTRUCT('longitude',
        COALESCE(TO_VARCHAR(lip.longitude), '<null>')),
      ARRAY_CONSTRUCT('loan_property_count',
        COALESCE(TO_VARCHAR(lip.loan_property_count), '<null>')),
      ARRAY_CONSTRUCT('dataset_code',
        COALESCE(lip.dataset_code::TEXT, '<null>'))
    )) AS payload_json
  FROM accepted a
  JOIN point_source_ids p
    ON p.row_key = a.row_key
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = a.bridge_build_id
   AND lip.loan_key = a.loan_key
   AND 'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:'
      || lip.build_id
      || ':'
      || lip.loan_key
      || ':'
      || COALESCE(lip.property_key::TEXT, '<null>')
      || ':'
      || COALESCE(lip.loan_issuance_property_key::TEXT, '<null>')
      = p.point_source_record_id
),
bridge_state_stats AS (
  SELECT
    row_key,
    COUNT_IF(bridge_property_state IS NULL
      OR bridge_property_state <> (SELECT expected_property_state FROM params))
      AS bridge_property_state_mismatch_rows
  FROM bridge_records
  GROUP BY row_key
),
master_records AS (
  SELECT
    row_key,
    20::NUMBER(9,0) AS role_order,
    'acris_master'::TEXT AS role,
    ARRAY_CONSTRUCT() AS parcel_ids,
    NULL::TEXT AS parcel_id,
    acris_master_source_record_id AS source_record_id,
    TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD') AS source_vintage,
    acris_master_filename AS source_filename,
    NULL::NUMBER(38,0) AS source_row_number,
    acris_master_raw_csv_sha256 AS upstream_sha256,
    TO_JSON(ARRAY_CONSTRUCT(
      ARRAY_CONSTRUCT('payload_contract',
        (SELECT payload_contract FROM params)),
      ARRAY_CONSTRUCT('source_record_class',
        (SELECT source_record_class FROM params)),
      ARRAY_CONSTRUCT('role', 'acris_master'),
      ARRAY_CONSTRUCT('source_table',
        'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER'),
      ARRAY_CONSTRUCT('source_record_id', acris_master_source_record_id),
      ARRAY_CONSTRUCT('source_vintage',
        TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD')),
      ARRAY_CONSTRUCT('raw_csv_sha256', acris_master_raw_csv_sha256),
      ARRAY_CONSTRUCT('filename', acris_master_filename),
      ARRAY_CONSTRUCT('document_id', document_id),
      ARRAY_CONSTRUCT('crfn', COALESCE(crfn, '<null>')),
      ARRAY_CONSTRUCT('doc_type', doc_type),
      ARRAY_CONSTRUCT('diagnostic_recorded_borough',
        COALESCE(TO_VARCHAR(diagnostic_recorded_borough), '<null>')),
      ARRAY_CONSTRUCT('document_date',
        COALESCE(TO_VARCHAR(document_date, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('recorded_date',
        COALESCE(TO_VARCHAR(recorded_date, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('recording_offset_days',
        COALESCE(TO_VARCHAR(recording_offset_days), '<null>')),
      ARRAY_CONSTRUCT('amount_cents_quantization',
        (SELECT amount_cents_quantization FROM params)),
      ARRAY_CONSTRUCT('amount_cents', TO_VARCHAR(amount_cents)),
      ARRAY_CONSTRUCT('loan_key', loan_key),
      ARRAY_CONSTRUCT('truth_plane', truth_plane)
    )) AS payload_json
  FROM accepted
),
party_records AS (
  SELECT
    row_key,
    30::NUMBER(9,0) AS role_order,
    'acris_party'::TEXT AS role,
    ARRAY_CONSTRUCT() AS parcel_ids,
    NULL::TEXT AS parcel_id,
    acris_party_source_record_id AS source_record_id,
    TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD') AS source_vintage,
    acris_party_filename AS source_filename,
    NULL::NUMBER(38,0) AS source_row_number,
    acris_party_raw_csv_sha256 AS upstream_sha256,
    TO_JSON(ARRAY_CONSTRUCT(
      ARRAY_CONSTRUCT('payload_contract',
        (SELECT payload_contract FROM params)),
      ARRAY_CONSTRUCT('source_record_class',
        (SELECT source_record_class FROM params)),
      ARRAY_CONSTRUCT('role', 'acris_party'),
      ARRAY_CONSTRUCT('source_table',
        'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES'),
      ARRAY_CONSTRUCT('source_record_id', acris_party_source_record_id),
      ARRAY_CONSTRUCT('source_vintage',
        TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD')),
      ARRAY_CONSTRUCT('raw_csv_sha256', acris_party_raw_csv_sha256),
      ARRAY_CONSTRUCT('filename', acris_party_filename),
      ARRAY_CONSTRUCT('document_id', document_id),
      ARRAY_CONSTRUCT('doc_type', doc_type),
      ARRAY_CONSTRUCT('lender_match_text', lender_match_text),
      ARRAY_CONSTRUCT('lender_party_type', lender_party_type),
      ARRAY_CONSTRUCT('originator_match_text', originator_match_text),
      ARRAY_CONSTRUCT('amount_cents', TO_VARCHAR(amount_cents)),
      ARRAY_CONSTRUCT('loan_key', loan_key),
      ARRAY_CONSTRUCT('truth_plane', truth_plane)
    )) AS payload_json
  FROM accepted
  WHERE truth_plane = 'round_exact_lender_party'
),
candidate_bbl_edges AS (
  SELECT DISTINCT
    a.row_key,
    a.loan_key,
    a.truth_plane,
    a.association_plane,
    a.mappluto_release,
    a.mappluto_release_dt,
    a.mappluto_variant,
    a.candidate_source_record_ids,
    a.candidate_geom_wkt_sha256s,
    candidate.value::TEXT AS candidate_bbl
  FROM accepted a,
    LATERAL FLATTEN(input => a.candidate_bbls) candidate
),
mappluto_records AS (
  SELECT DISTINCT
    c.row_key,
    50::NUMBER(9,0) AS role_order,
    'mappluto_candidate'::TEXT AS role,
    ARRAY_CONSTRUCT(c.candidate_bbl) AS parcel_ids,
    c.candidate_bbl AS parcel_id,
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
      AS source_record_id,
    TO_VARCHAR(p.release_dt, 'YYYY-MM-DD') AS source_vintage,
    p.source_filename AS source_filename,
    p.source_row_number::NUMBER(38,0) AS source_row_number,
    p.geom_wkt_sha256 AS upstream_sha256,
    TO_JSON(ARRAY_CONSTRUCT(
      ARRAY_CONSTRUCT('payload_contract',
        (SELECT payload_contract FROM params)),
      ARRAY_CONSTRUCT('source_record_class',
        (SELECT source_record_class FROM params)),
      ARRAY_CONSTRUCT('role', 'mappluto_candidate'),
      ARRAY_CONSTRUCT('source_table',
        'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES'),
      ARRAY_CONSTRUCT('source_record_id',
        'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:'
          || p.release
          || ':'
          || TO_VARCHAR(p.release_dt)
          || ':'
          || p.variant
          || ':'
          || COALESCE(p.bbl_key, '<null>')
          || ':'
          || COALESCE(TO_VARCHAR(p.source_row_number::NUMBER(38,0)), '<null>')),
      ARRAY_CONSTRUCT('source_vintage',
        TO_VARCHAR(p.release_dt, 'YYYY-MM-DD')),
      ARRAY_CONSTRUCT('release', p.release),
      ARRAY_CONSTRUCT('release_dt', TO_VARCHAR(p.release_dt, 'YYYY-MM-DD')),
      ARRAY_CONSTRUCT('variant', p.variant),
      ARRAY_CONSTRUCT('bbl_key', p.bbl_key),
      ARRAY_CONSTRUCT('source_filename', p.source_filename),
      ARRAY_CONSTRUCT('source_row_number',
        TO_VARCHAR(p.source_row_number::NUMBER(38,0))),
      ARRAY_CONSTRUCT('geom_wkt_sha256', p.geom_wkt_sha256),
      ARRAY_CONSTRUCT('geometry_contract_version',
        (SELECT mappluto_geometry_contract_version FROM params)),
      ARRAY_CONSTRUCT('loan_key', c.loan_key),
      ARRAY_CONSTRUCT('truth_plane', c.truth_plane),
      ARRAY_CONSTRUCT('association_plane', c.association_plane)
    )) AS payload_json,
    ARRAY_CONTAINS(
      ('EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:'
        || p.release
        || ':'
        || TO_VARCHAR(p.release_dt)
        || ':'
        || p.variant
        || ':'
        || COALESCE(p.bbl_key, '<null>')
        || ':'
        || COALESCE(TO_VARCHAR(p.source_row_number::NUMBER(38,0)), '<null>')
      )::VARIANT,
      c.candidate_source_record_ids
    ) AS locator_was_in_first_stage,
    ARRAY_CONTAINS(p.geom_wkt_sha256::VARIANT, c.candidate_geom_wkt_sha256s)
      AS geometry_hash_was_in_first_stage
  FROM candidate_bbl_edges c
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES p
    ON p.release = c.mappluto_release
   AND p.release_dt = c.mappluto_release_dt
   AND p.variant = c.mappluto_variant
   AND p.bbl_key = c.candidate_bbl
  WHERE p.bbl_key_status = 'valid'
    AND p.key_validation_status = 'valid'
    AND p.bbl_key IS NOT NULL
    AND p.geom_wkt_sha256 IS NOT NULL
    AND p.source_filename IS NOT NULL
    AND p.source_row_number IS NOT NULL
),
acris_legal_entries AS (
  SELECT
    a.row_key,
    a.loan_key,
    a.truth_plane,
    a.association_plane,
    a.document_id,
    a.crfn,
    a.doc_type,
    a.acris_release_dt,
    a.originationdate,
    a.document_date,
    a.recorded_date,
    a.amount_cents,
    a.lender_match_text,
    a.originator_match_text,
    entry.value AS legal_entry,
    entry.value:source_record_id::TEXT AS upstream_source_record_id,
    entry.value:raw_csv_sha256::TEXT AS raw_csv_sha256,
    entry.value:filename::TEXT AS filename,
    entry.value:legal_bbl::TEXT AS legal_bbl,
    entry.value:filed_borough::NUMBER(38,0) AS filed_borough
  FROM accepted a,
    LATERAL FLATTEN(input => a.acris_legal_source_records) entry
),
legal_records AS (
  SELECT DISTINCT
    row_key,
    40::NUMBER(9,0) AS role_order,
    'acris_legal'::TEXT AS role,
    ARRAY_CONSTRUCT(legal_bbl) AS parcel_ids,
    legal_bbl AS parcel_id,
    'derived:canon:h7:acris_legal_edge:'
      || upstream_source_record_id
      || ':legal_bbl:'
      || legal_bbl AS source_record_id,
    TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD') AS source_vintage,
    filename AS source_filename,
    NULL::NUMBER(38,0) AS source_row_number,
    raw_csv_sha256 AS upstream_sha256,
    TO_JSON(ARRAY_CONSTRUCT(
      ARRAY_CONSTRUCT('payload_contract',
        (SELECT payload_contract FROM params)),
      ARRAY_CONSTRUCT('source_record_class',
        (SELECT source_record_class FROM params)),
      ARRAY_CONSTRUCT('role', 'acris_legal'),
      ARRAY_CONSTRUCT('source_table',
        'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS'),
      ARRAY_CONSTRUCT('source_record_id',
        'derived:canon:h7:acris_legal_edge:'
          || upstream_source_record_id
          || ':legal_bbl:'
          || legal_bbl),
      ARRAY_CONSTRUCT('upstream_source_record_id', upstream_source_record_id),
      ARRAY_CONSTRUCT('source_vintage',
        TO_VARCHAR(acris_release_dt, 'YYYY-MM-DD')),
      ARRAY_CONSTRUCT('raw_csv_sha256', raw_csv_sha256),
      ARRAY_CONSTRUCT('filename', filename),
      ARRAY_CONSTRUCT('document_id', document_id),
      ARRAY_CONSTRUCT('crfn', COALESCE(crfn, '<null>')),
      ARRAY_CONSTRUCT('doc_type', doc_type),
      ARRAY_CONSTRUCT('amount_cents', TO_VARCHAR(amount_cents)),
      ARRAY_CONSTRUCT('originationdate',
        COALESCE(TO_VARCHAR(originationdate, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('document_date',
        COALESCE(TO_VARCHAR(document_date, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('recorded_date',
        COALESCE(TO_VARCHAR(recorded_date, 'YYYY-MM-DD'), '<null>')),
      ARRAY_CONSTRUCT('lender_match_text',
        COALESCE(lender_match_text, '<null>')),
      ARRAY_CONSTRUCT('originator_match_text',
        COALESCE(originator_match_text, '<null>')),
      ARRAY_CONSTRUCT('legal_bbl', legal_bbl),
      ARRAY_CONSTRUCT('filed_borough', TO_VARCHAR(filed_borough)),
      ARRAY_CONSTRUCT('loan_key', loan_key),
      ARRAY_CONSTRUCT('truth_plane', truth_plane),
      ARRAY_CONSTRUCT('association_plane', association_plane)
    )) AS payload_json
  FROM acris_legal_entries
),
truth_parcel_edges AS (
  SELECT DISTINCT
    a.row_key,
    truth.value::TEXT AS truth_bbl
  FROM accepted a,
    LATERAL FLATTEN(input => a.truth_bbls) truth
),
source_record_payloads AS (
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    TRUE AS locator_was_in_first_stage,
    TRUE AS geometry_hash_was_in_first_stage
  FROM bridge_records
  UNION ALL
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    TRUE AS locator_was_in_first_stage,
    TRUE AS geometry_hash_was_in_first_stage
  FROM master_records
  UNION ALL
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    TRUE AS locator_was_in_first_stage,
    TRUE AS geometry_hash_was_in_first_stage
  FROM party_records
  UNION ALL
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    TRUE AS locator_was_in_first_stage,
    TRUE AS geometry_hash_was_in_first_stage
  FROM legal_records
  UNION ALL
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    locator_was_in_first_stage,
    geometry_hash_was_in_first_stage
  FROM mappluto_records
),
source_record_wrappers AS (
  SELECT
    row_key,
    role_order,
    role,
    parcel_ids,
    parcel_id,
    source_record_id,
    source_vintage,
    source_filename,
    source_row_number,
    upstream_sha256,
    payload_json,
    LENGTH(payload_json) AS payload_utf8_bytes,
    BASE64_ENCODE(TO_BINARY(payload_json, 'UTF-8'))
      AS source_record_bytes_base64,
    locator_was_in_first_stage,
    geometry_hash_was_in_first_stage,
    OBJECT_CONSTRUCT_KEEP_NULL(
      'role', role,
      'parcel_ids', parcel_ids,
      'source_record', OBJECT_CONSTRUCT_KEEP_NULL(
        'source_record_id', source_record_id,
        'source_vintage', source_vintage,
        'record_blake3', ''
      ),
      'source_record_bytes_base64',
        BASE64_ENCODE(TO_BINARY(payload_json, 'UTF-8'))
    ) AS source_record_wrapper
  FROM source_record_payloads
),
source_record_arrays AS (
  SELECT
    row_key,
    ARRAY_AGG(source_record_wrapper) WITHIN GROUP (
      ORDER BY role_order, COALESCE(parcel_id, ''), source_record_id
    ) AS source_records,
    COUNT(*) AS source_record_count,
    COUNT(DISTINCT source_record_id) AS distinct_source_record_ids,
    COUNT_IF(role = 'bridge_loan') AS bridge_source_record_count,
    COUNT_IF(role = 'acris_master') AS acris_master_source_record_count,
    COUNT_IF(role = 'acris_party') AS acris_party_source_record_count,
    COUNT_IF(role = 'acris_legal') AS acris_legal_source_record_count,
    COUNT_IF(role = 'mappluto_candidate') AS mappluto_source_record_count,
    MIN(payload_utf8_bytes) AS min_source_record_payload_utf8_bytes,
    MAX(payload_utf8_bytes) AS max_source_record_payload_utf8_bytes,
    SUM(payload_utf8_bytes) AS total_source_record_payload_utf8_bytes,
    MAX(LENGTH(source_record_bytes_base64))
      AS max_source_record_payload_base64_chars,
    COUNT_IF(source_record_id IS NULL OR source_record_id = '')
      AS missing_source_record_ids,
    COUNT_IF(source_vintage IS NULL OR source_vintage = '')
      AS missing_source_vintages,
    COUNT_IF(role IN ('acris_master', 'acris_party', 'acris_legal',
      'mappluto_candidate')
      AND (upstream_sha256 IS NULL
        OR NOT upstream_sha256 RLIKE '^[0-9a-f]{64}$'))
      AS source_hash_format_failures,
    COUNT_IF(source_record_bytes_base64 IS NULL
      OR NOT source_record_bytes_base64 RLIKE
        '^([A-Za-z0-9+/]{4})*([A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$')
      AS base64_format_failures,
    COUNT_IF(payload_utf8_bytes <= LENGTH(source_record_id))
      AS locator_only_payload_failures,
    COUNT_IF(role IN ('acris_legal', 'mappluto_candidate')
      AND ARRAY_SIZE(parcel_ids) <> 1)
      AS single_parcel_wrapper_failures,
    COUNT_IF(role IN ('bridge_loan', 'acris_master', 'acris_party')
      AND ARRAY_SIZE(parcel_ids) <> 0)
      AS empty_parcel_wrapper_failures,
    COUNT_IF(NOT locator_was_in_first_stage)
      AS mappluto_locator_binding_failures,
    COUNT_IF(NOT geometry_hash_was_in_first_stage)
      AS mappluto_geometry_hash_binding_failures
  FROM source_record_wrappers
  GROUP BY row_key
),
legal_parcel_support AS (
  SELECT
    row_key,
    COUNT(DISTINCT parcel_id) AS legal_supported_parcels
  FROM source_record_wrappers
  WHERE role = 'acris_legal'
  GROUP BY row_key
),
candidate_parcel_support AS (
  SELECT
    row_key,
    COUNT(DISTINCT parcel_id) AS candidate_supported_parcels
  FROM source_record_wrappers
  WHERE role = 'mappluto_candidate'
  GROUP BY row_key
),
legal_missing AS (
  SELECT
    t.row_key,
    COUNT(DISTINCT t.truth_bbl) AS missing_truth_parcels
  FROM truth_parcel_edges t
  LEFT JOIN source_record_wrappers w
    ON w.row_key = t.row_key
   AND w.role = 'acris_legal'
   AND w.parcel_id = t.truth_bbl
  WHERE w.source_record_id IS NULL
  GROUP BY t.row_key
),
legal_extra AS (
  SELECT
    w.row_key,
    COUNT(DISTINCT w.parcel_id) AS extra_truth_parcels
  FROM source_record_wrappers w
  LEFT JOIN truth_parcel_edges t
    ON t.row_key = w.row_key
   AND t.truth_bbl = w.parcel_id
  WHERE w.role = 'acris_legal'
    AND t.truth_bbl IS NULL
  GROUP BY w.row_key
),
candidate_missing AS (
  SELECT
    c.row_key,
    COUNT(DISTINCT c.candidate_bbl) AS missing_candidate_parcels
  FROM candidate_bbl_edges c
  LEFT JOIN source_record_wrappers w
    ON w.row_key = c.row_key
   AND w.role = 'mappluto_candidate'
   AND w.parcel_id = c.candidate_bbl
  WHERE w.source_record_id IS NULL
  GROUP BY c.row_key
),
candidate_extra AS (
  SELECT
    w.row_key,
    COUNT(DISTINCT w.parcel_id) AS extra_candidate_parcels
  FROM source_record_wrappers w
  LEFT JOIN candidate_bbl_edges c
    ON c.row_key = w.row_key
   AND c.candidate_bbl = w.parcel_id
  WHERE w.role = 'mappluto_candidate'
    AND c.candidate_bbl IS NULL
  GROUP BY w.row_key
),
representative_borough_edges AS (
  SELECT
    row_key,
    edge.value:filed_county::TEXT AS filed_county,
    edge.value:filed_borough::NUMBER(38,0) AS filed_borough,
    edge.value:filed_borough::NUMBER(38,0) AS legal_borough,
    ROW_NUMBER() OVER (
      PARTITION BY row_key
      ORDER BY edge.value:filed_county::TEXT,
        edge.value:filed_borough::NUMBER(38,0)
    ) AS edge_rank
  FROM accepted,
    LATERAL FLATTEN(input => filed_county_borough_edges) edge
),
accepted_borough_edge_arrays AS (
  SELECT
    row_key,
    ARRAY_AGG(
      OBJECT_CONSTRUCT_KEEP_NULL(
        'filed_county', edge.value:filed_county::TEXT,
        'filed_borough', edge.value:filed_borough::NUMBER(38,0),
        'legal_borough', edge.value:filed_borough::NUMBER(38,0)
      )
    ) WITHIN GROUP (
      ORDER BY edge.value:filed_county::TEXT,
        edge.value:filed_borough::NUMBER(38,0)
    ) AS accepted_borough_edges
  FROM accepted,
    LATERAL FLATTEN(input => filed_county_borough_edges) edge
  GROUP BY row_key
),
row_outputs AS (
  SELECT
    a.*,
    OBJECT_CONSTRUCT_KEEP_NULL(
      'release', a.mappluto_release,
      'release_dt', TO_VARCHAR(a.mappluto_release_dt, 'YYYY-MM-DD'),
      'variant', a.mappluto_variant,
      'geometry_contract_version',
        (SELECT mappluto_geometry_contract_version FROM params)
    ) AS candidate_release,
    b.filed_county,
    b.filed_borough,
    b.legal_borough,
    be.accepted_borough_edges,
    IFF(a.diagnostic_county_fips IS NOT NULL
      AND ARRAY_SIZE(a.diagnostic_county_fips) = 1,
      a.diagnostic_county_fips[0]::TEXT,
      NULL) AS geocoded_county_fips,
    OBJECT_CONSTRUCT_KEEP_NULL(
      'originatorname', a.distinct_counts:originatorname::NUMBER(38,0),
      'originator_match_text',
        a.distinct_counts:originator_match_text::NUMBER(38,0),
      'originationdate', a.distinct_counts:originationdate::NUMBER(38,0),
      'originalloanamount',
        a.distinct_counts:originalloanamount::NUMBER(38,0),
      'filed_borough', a.distinct_counts:filed_borough::NUMBER(38,0)
    ) AS loan_field_distinct_counts,
    COALESCE(s.source_records, ARRAY_CONSTRUCT()) AS source_records,
    COALESCE(s.source_record_count, 0) AS source_record_count,
    COALESCE(s.distinct_source_record_ids, 0) AS distinct_source_record_ids,
    COALESCE(s.bridge_source_record_count, 0) AS bridge_source_record_count,
    COALESCE(s.acris_master_source_record_count, 0)
      AS acris_master_source_record_count,
    COALESCE(s.acris_party_source_record_count, 0)
      AS acris_party_source_record_count,
    COALESCE(s.acris_legal_source_record_count, 0)
      AS acris_legal_source_record_count,
    COALESCE(s.mappluto_source_record_count, 0)
      AS mappluto_source_record_count,
    COALESCE(s.min_source_record_payload_utf8_bytes, 0)
      AS min_source_record_payload_utf8_bytes,
    COALESCE(s.max_source_record_payload_utf8_bytes, 0)
      AS max_source_record_payload_utf8_bytes,
    COALESCE(s.total_source_record_payload_utf8_bytes, 0)
      AS total_source_record_payload_utf8_bytes,
    COALESCE(s.max_source_record_payload_base64_chars, 0)
      AS max_source_record_payload_base64_chars,
    COALESCE(s.missing_source_record_ids, 0) AS missing_source_record_ids,
    COALESCE(s.missing_source_vintages, 0) AS missing_source_vintages,
    COALESCE(s.source_hash_format_failures, 0)
      AS source_hash_format_failures,
    COALESCE(s.base64_format_failures, 0) AS base64_format_failures,
    COALESCE(s.locator_only_payload_failures, 0)
      AS locator_only_payload_failures,
    COALESCE(s.single_parcel_wrapper_failures, 0)
      AS single_parcel_wrapper_failures,
    COALESCE(s.empty_parcel_wrapper_failures, 0)
      AS empty_parcel_wrapper_failures,
    COALESCE(s.mappluto_locator_binding_failures, 0)
      AS mappluto_locator_binding_failures,
    COALESCE(s.mappluto_geometry_hash_binding_failures, 0)
      AS mappluto_geometry_hash_binding_failures,
    COALESCE(bs.bridge_property_state_mismatch_rows, 0)
      AS bridge_property_state_mismatch_rows,
    COALESCE(lps.legal_supported_parcels, 0) AS legal_supported_parcels,
    COALESCE(cps.candidate_supported_parcels, 0)
      AS candidate_supported_parcels,
    COALESCE(lm.missing_truth_parcels, 0) AS missing_truth_parcels,
    COALESCE(le.extra_truth_parcels, 0) AS extra_truth_parcels,
    COALESCE(cm.missing_candidate_parcels, 0) AS missing_candidate_parcels,
    COALESCE(ce.extra_candidate_parcels, 0) AS extra_candidate_parcels
  FROM accepted a
  JOIN representative_borough_edges b
    ON b.row_key = a.row_key
   AND b.edge_rank = 1
  JOIN accepted_borough_edge_arrays be
    ON be.row_key = a.row_key
  LEFT JOIN source_record_arrays s
    ON s.row_key = a.row_key
  LEFT JOIN bridge_state_stats bs
    ON bs.row_key = a.row_key
  LEFT JOIN legal_parcel_support lps
    ON lps.row_key = a.row_key
  LEFT JOIN candidate_parcel_support cps
    ON cps.row_key = a.row_key
  LEFT JOIN legal_missing lm
    ON lm.row_key = a.row_key
  LEFT JOIN legal_extra le
    ON le.row_key = a.row_key
  LEFT JOIN candidate_missing cm
    ON cm.row_key = a.row_key
  LEFT JOIN candidate_extra ce
    ON ce.row_key = a.row_key
),
output_stats AS (
  SELECT
    COUNT(*) AS output_release_rows,
    COUNT(DISTINCT loan_key) AS output_accepted_loans,
    COUNT(DISTINCT row_key) AS unique_output_release_rows,
    MIN(source_record_count) AS min_source_record_count,
    MAX(source_record_count) AS max_source_record_count,
    MAX(max_source_record_payload_utf8_bytes) AS max_payload_utf8_bytes,
    MAX(max_source_record_payload_base64_chars) AS max_payload_base64_chars,
    MAX(total_source_record_payload_utf8_bytes) AS max_row_payload_utf8_bytes,
    COUNT_IF(source_record_count > (SELECT source_record_cap_per_release_row
                                   FROM params))
      AS source_record_cap_failures,
    COUNT_IF(max_source_record_payload_utf8_bytes
      > (SELECT source_record_payload_utf8_cap FROM params))
      AS payload_utf8_cap_failures,
    COUNT_IF(total_source_record_payload_utf8_bytes
      > (SELECT row_payload_utf8_cap FROM params))
      AS row_payload_utf8_cap_failures,
    COUNT_IF(source_record_count <> distinct_source_record_ids)
      AS source_record_id_uniqueness_failures,
    COUNT_IF(source_records IS NULL OR TYPEOF(source_records) <> 'ARRAY')
      AS source_record_array_failures,
    COUNT_IF(missing_source_record_ids <> 0
      OR missing_source_vintages <> 0
      OR source_hash_format_failures <> 0
      OR base64_format_failures <> 0
      OR locator_only_payload_failures <> 0
      OR single_parcel_wrapper_failures <> 0
      OR empty_parcel_wrapper_failures <> 0
      OR mappluto_locator_binding_failures <> 0
      OR mappluto_geometry_hash_binding_failures <> 0)
      AS source_wrapper_failures,
    COUNT_IF(bridge_property_state_mismatch_rows <> 0)
      AS bridge_property_state_mismatch_rows,
    COUNT_IF(bridge_source_record_count = 0
      OR acris_master_source_record_count <> 1
      OR acris_legal_source_record_count = 0
      OR mappluto_source_record_count <> candidate_bbl_count
      OR (truth_plane = 'round_exact_lender_party'
        AND acris_party_source_record_count <> 1)
      OR (truth_plane <> 'round_exact_lender_party'
        AND acris_party_source_record_count <> 0))
      AS source_role_coverage_failures,
    COUNT_IF(legal_supported_parcels <> truth_bbl_count
      OR missing_truth_parcels <> 0
      OR extra_truth_parcels <> 0)
      AS legal_parcel_union_failures,
    COUNT_IF(candidate_supported_parcels <> candidate_bbl_count
      OR missing_candidate_parcels <> 0
      OR extra_candidate_parcels <> 0)
      AS candidate_parcel_union_failures,
    COUNT_IF(candidate_bbl_count = 0 AND mappluto_source_record_count <> 0)
      AS zero_candidate_mappluto_leakage_rows,
    COUNT_IF(candidate_bbl_count > 0 AND mappluto_source_record_count = 0)
      AS nonzero_candidate_missing_mappluto_rows
  FROM row_outputs
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT 'pip_block_population_query_id_sentinel_unsubstituted'
        AS failure_reason,
      (SELECT pip_block_population_query_id FROM params)
        = (SELECT pip_block_population_query_id_unbound_marker
           FROM sentinel_markers) AS failed
    UNION ALL
    SELECT 'input_result_empty',
      (SELECT input_release_rows FROM input_stats) = 0
    UNION ALL
    SELECT 'input_result_exceeds_bound',
      (SELECT input_release_rows FROM input_stats)
        > (SELECT input_row_cap FROM params)
    UNION ALL
    SELECT 'input_release_row_count_not_142',
      (SELECT input_release_rows FROM input_stats)
        <> (SELECT expected_release_rows FROM params)
    UNION ALL
    SELECT 'input_accepted_loan_count_not_71',
      (SELECT accepted_loans FROM input_stats)
        <> (SELECT expected_accepted_loans FROM params)
    UNION ALL
    SELECT 'input_duplicate_subject_release',
      (SELECT input_release_rows FROM input_stats)
        <> (SELECT distinct_release_rows FROM input_stats)
    UNION ALL
    SELECT 'input_release_pin_count_mismatch',
      (SELECT distinct_release_pins FROM input_stats)
        <> (SELECT expected_release_count FROM params)
    UNION ALL
    SELECT 'input_contract_mismatch',
      (SELECT input_contract_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_bridge_build_mismatch',
      (SELECT bridge_build_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_collateral_scope_mismatch',
      (SELECT collateral_scope_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_acris_release_dt_mismatch',
      (SELECT acris_release_dt_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_candidate_bbl_count_mismatch',
      (SELECT candidate_bbl_count_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_candidate_source_count_mismatch',
      (SELECT candidate_source_count_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_truth_bbl_count_mismatch',
      (SELECT truth_bbl_count_mismatch_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_array_mapping_failure',
      (SELECT array_mapping_failures FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_zero_candidate_array_leakage',
      (SELECT zero_candidate_array_leakage_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_candidate_cap_exceeded',
      (SELECT candidate_cap_failures FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_reached_truth_accounting_failure',
      (SELECT reached_truth_accounting_failures FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_release_pin_failure',
      (SELECT release_pin_failures FROM input_stats) <> 0
    UNION ALL
    SELECT 'input_missing_round_party_source',
      (SELECT missing_round_party_rows FROM input_stats) <> 0
    UNION ALL
    SELECT 'output_row_count_not_142',
      (SELECT output_release_rows FROM output_stats)
        <> (SELECT expected_release_rows FROM params)
    UNION ALL
    SELECT 'output_row_count_exceeds_bound',
      (SELECT output_release_rows FROM output_stats)
        > (SELECT output_row_cap FROM params)
    UNION ALL
    SELECT 'output_accepted_loan_count_not_71',
      (SELECT output_accepted_loans FROM output_stats)
        <> (SELECT expected_accepted_loans FROM params)
    UNION ALL
    SELECT 'output_duplicate_subject_release',
      (SELECT output_release_rows FROM output_stats)
        <> (SELECT unique_output_release_rows FROM output_stats)
    UNION ALL
    SELECT 'output_source_record_cap_exceeded',
      (SELECT source_record_cap_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_payload_utf8_cap_exceeded',
      (SELECT payload_utf8_cap_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_row_payload_utf8_cap_exceeded',
      (SELECT row_payload_utf8_cap_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_source_record_id_not_unique',
      (SELECT source_record_id_uniqueness_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_source_record_array_failure',
      (SELECT source_record_array_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_source_wrapper_failure',
      (SELECT source_wrapper_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'input_bridge_property_state_mismatch',
      (SELECT bridge_property_state_mismatch_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_source_role_coverage_failure',
      (SELECT source_role_coverage_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_legal_parcel_union_failure',
      (SELECT legal_parcel_union_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_candidate_parcel_union_failure',
      (SELECT candidate_parcel_union_failures FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_zero_candidate_mappluto_leakage',
      (SELECT zero_candidate_mappluto_leakage_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'output_nonzero_candidate_missing_mappluto',
      (SELECT nonzero_candidate_missing_mappluto_rows FROM output_stats) <> 0
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
    f.failure_reason AS refusal_reason,
    (SELECT pip_block_population_query_id FROM params)
      AS pip_block_population_query_id,
    (SELECT payload_contract FROM params) AS payload_contract,
    (SELECT source_record_class FROM params) AS source_record_class,
    NULL::TEXT AS accepted_truth_query_id,
    NULL::TEXT AS loan_key,
    NULL::TEXT AS document_id,
    NULL::TEXT AS truth_plane,
    NULL::TEXT AS association_plane,
    NULL::NUMBER(38,0) AS accepted_plane_eligible_loans,
    NULL::NUMBER(38,0) AS accepted_plane_legal_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_legal_confirmed_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_accepted_loans,
    NULL::NUMBER(38,0) AS accepted_plane_ambiguous_loans,
    NULL::NUMBER(38,0) AS accepted_plane_candidate_without_legal_loans,
    NULL::NUMBER(38,0) AS accepted_plane_no_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_plane_selected_multi_parcel_loans,
    NULL::TEXT AS mappluto_release,
    NULL::DATE AS mappluto_release_dt,
    NULL::TEXT AS mappluto_variant,
    NULL::VARIANT AS candidate_release,
    NULL::TEXT AS property_state,
    NULL::TEXT AS filed_county,
    NULL::NUMBER(38,0) AS filed_borough,
    NULL::NUMBER(38,0) AS legal_borough,
    NULL::VARIANT AS accepted_borough_edges,
    NULL::TEXT AS geocoded_county_fips,
    NULL::TEXT AS doc_type,
    NULL::DATE AS originationdate,
    NULL::NUMBER(38,0) AS amount_cents,
    NULL::BOOLEAN AS is_round_100k_lattice,
    NULL::TEXT AS originatorname,
    NULL::TEXT AS originator_match_text,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::VARIANT AS loan_field_distinct_counts,
    NULL::VARIANT AS truth_parcels,
    NULL::VARIANT AS candidate_parcels,
    NULL::TEXT AS reach_status,
    NULL::TEXT AS reach_reason,
    NULL::VARIANT AS source_records,
    NULL::NUMBER(38,0) AS source_record_count,
    NULL::NUMBER(38,0) AS bridge_source_record_count,
    NULL::NUMBER(38,0) AS acris_master_source_record_count,
    NULL::NUMBER(38,0) AS acris_party_source_record_count,
    NULL::NUMBER(38,0) AS acris_legal_source_record_count,
    NULL::NUMBER(38,0) AS mappluto_source_record_count,
    NULL::NUMBER(38,0) AS min_source_record_payload_utf8_bytes,
    NULL::NUMBER(38,0) AS max_source_record_payload_utf8_bytes,
    NULL::NUMBER(38,0) AS total_source_record_payload_utf8_bytes,
    NULL::NUMBER(38,0) AS max_source_record_payload_base64_chars,
    NULL::NUMBER(38,0) AS candidate_bbl_count,
    NULL::NUMBER(38,0) AS truth_bbl_count,
    NULL::NUMBER(38,0) AS reached_truth_bbls,
    NULL::NUMBER(38,0) AS whole_accepted_loans,
    NULL::NUMBER(38,0) AS whole_release_rows,
    NULL::NUMBER(38,0) AS whole_zero_candidate_release_rows
  FROM guard_failures f
  CROSS JOIN guard_summary g
),
accepted_output AS (
  SELECT
    (SELECT output_row_contract FROM params) AS row_contract,
    'source_record_payload_release_row'::TEXT AS row_kind,
    g.guard_status,
    g.refusal_reason,
    (SELECT pip_block_population_query_id FROM params)
      AS pip_block_population_query_id,
    (SELECT payload_contract FROM params) AS payload_contract,
    (SELECT source_record_class FROM params) AS source_record_class,
    r.accepted_truth_query_id,
    r.loan_key,
    r.document_id,
    r.truth_plane,
    r.association_plane,
    r.accepted_plane_eligible_loans,
    r.accepted_plane_legal_candidate_loans,
    r.accepted_plane_legal_confirmed_candidate_loans,
    r.accepted_plane_accepted_loans,
    r.accepted_plane_ambiguous_loans,
    r.accepted_plane_candidate_without_legal_loans,
    r.accepted_plane_no_candidate_loans,
    r.accepted_plane_selected_multi_parcel_loans,
    r.mappluto_release,
    r.mappluto_release_dt,
    r.mappluto_variant,
    r.candidate_release,
    (SELECT expected_property_state FROM params) AS property_state,
    r.filed_county,
    r.filed_borough,
    r.legal_borough,
    r.accepted_borough_edges,
    r.geocoded_county_fips,
    r.doc_type,
    r.originationdate,
    r.amount_cents,
    MOD(r.amount_cents, 10000000) = 0 AS is_round_100k_lattice,
    r.originatorname,
    r.originator_match_text,
    r.lender_match_text,
    r.lender_party_type,
    r.loan_field_distinct_counts,
    r.truth_bbls AS truth_parcels,
    r.candidate_bbls AS candidate_parcels,
    r.reach_status,
    'pip_block_candidate_release_scored_against_same_accepted_h7_truth'
      AS reach_reason,
    r.source_records,
    r.source_record_count,
    r.bridge_source_record_count,
    r.acris_master_source_record_count,
    r.acris_party_source_record_count,
    r.acris_legal_source_record_count,
    r.mappluto_source_record_count,
    r.min_source_record_payload_utf8_bytes,
    r.max_source_record_payload_utf8_bytes,
    r.total_source_record_payload_utf8_bytes,
    r.max_source_record_payload_base64_chars,
    r.candidate_bbl_count,
    r.truth_bbl_count,
    r.reached_truth_bbls,
    r.whole_accepted_loans,
    r.whole_release_rows,
    r.whole_zero_candidate_release_rows
  FROM row_outputs r
  CROSS JOIN guard_summary g
  WHERE g.guard_status = 'ok'
)
SELECT *
FROM accepted_output
UNION ALL
SELECT *
FROM guard_output
ORDER BY row_kind, truth_plane, loan_key, mappluto_release;
