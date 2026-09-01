-- Appendix H.7 Stage 3 flat ACRIS LEGALS residual shard SQL.
--
-- This is the third acquisition-side single-shard query for H.7. It consumes
-- one successful Stage-2 h7_stage2_master_party_candidate_row.v1 RESULT_SCAN
-- shard and emits flat scalar rows at candidate/no-candidate/no-legal/legal or
-- guard-failure grain. It first binds ACRIS LEGALS to the shard's distinct
-- loan/document/filed-borough keys, then joins the two pinned MapPLUTO geom-v3
-- scoring releases only for legal/no-legal rows. Plane/shard/denominator
-- columns repeat on every upstream row; MapPLUTO pins are null when they are
-- not applicable. It deliberately performs no OBJECT/ARRAY aggregation.
--
-- Byte-substitute only the operational placeholder literals in residual_params
-- and RESULT_SCAN:
--   '__BD7BCP_H7_STAGE2_CANDIDATE_QUERY_ID__'
--   '__BD7BCP_H7_TRUTH_PLANE__'
--   '__BD7BCP_H7_SHARD_COUNT__'
--   '__BD7BCP_H7_SHARD_INDEX__'
--   '__BD7BCP_H7_CURRENT_BRIDGE_BUILD_ID__'
-- Do not rewrite residual_sentinel_markers; those split marker expressions are
-- immutable unbound guards.

WITH
residual_params AS (
  SELECT
    '__BD7BCP_H7_STAGE2_CANDIDATE_QUERY_ID__'::TEXT
      AS stage2_candidate_query_id,
    '__BD7BCP_H7_TRUTH_PLANE__'::TEXT AS selected_truth_plane,
    '__BD7BCP_H7_SHARD_COUNT__'::TEXT AS shard_count_literal,
    '__BD7BCP_H7_SHARD_INDEX__'::TEXT AS shard_index_literal,
    TRY_TO_NUMBER('__BD7BCP_H7_SHARD_COUNT__')::NUMBER(38,0)
      AS shard_count,
    TRY_TO_NUMBER('__BD7BCP_H7_SHARD_INDEX__')::NUMBER(38,0)
      AS shard_index,
    'h7_stage2_master_party_candidate_row.v1'::TEXT
      AS expected_stage2_row_contract,
    'h7_stage3_legal_residual_row.v1'::TEXT AS output_row_contract,
    '__BD7BCP_H7_CURRENT_BRIDGE_BUILD_ID__'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    256::NUMBER(38,0) AS max_shard_count,
    100000::NUMBER(38,0) AS max_stage2_rows
),
residual_sentinel_markers AS (
  SELECT
    ('__BD7BCP_H7_' || 'STAGE2_CANDIDATE_QUERY_ID__')::TEXT
      AS stage2_candidate_query_id_unbound_marker,
    ('__BD7BCP_H7_' || 'TRUTH_PLANE__')::TEXT
      AS truth_plane_unbound_marker,
    ('__BD7BCP_H7_' || 'SHARD_COUNT__')::TEXT
      AS shard_count_unbound_marker,
    ('__BD7BCP_H7_' || 'SHARD_INDEX__')::TEXT
      AS shard_index_unbound_marker,
    ('__BD7BCP_H7_' || 'CURRENT_BRIDGE_BUILD_ID__')::TEXT
      AS bridge_build_id_unbound_marker
),
mappluto_release_pins AS (
  SELECT
    column1::TEXT AS mappluto_release,
    TO_DATE(column2::TEXT) AS mappluto_release_dt,
    column3::TEXT AS mappluto_variant,
    'geom-v3'::TEXT AS mappluto_geometry_plane
  FROM VALUES
    ('26v1', '2026-05-01', 'shoreline_clipped'),
    ('26v2', '2026-08-01', 'shoreline_clipped')
),
stage2_rows AS (
  SELECT
    row_contract::TEXT AS stage2_row_contract,
    row_kind::TEXT AS stage2_row_kind,
    guard_status::TEXT AS stage2_guard_status,
    guard_failure_reason::TEXT AS stage2_guard_failure_reason,
    upstream_loan_parameter_query_id::TEXT AS upstream_loan_parameter_query_id,
    selected_truth_plane::TEXT AS selected_truth_plane,
    shard_count::NUMBER(38,0) AS shard_count,
    shard_index::NUMBER(38,0) AS shard_index,
    shard_partition_expression::TEXT AS shard_partition_expression,
    bridge_build_id::TEXT AS bridge_build_id,
    TRY_TO_DATE(acris_release_dt::TEXT) AS acris_release_dt,
    property_state::TEXT AS property_state,
    collateral_scope::TEXT AS collateral_scope,
    amount_cents_quantization::TEXT AS amount_cents_quantization,
    round_amount_lattice_cents::NUMBER(38,0) AS round_amount_lattice_cents,
    max_recording_offset_days::NUMBER(38,0) AS max_recording_offset_days,
    whole_plane_eligible_loans::NUMBER(38,0) AS whole_plane_eligible_loans,
    whole_plane_loan_parameter_rows::NUMBER(38,0)
      AS whole_plane_loan_parameter_rows,
    shard_loan_parameter_rows::NUMBER(38,0) AS shard_loan_parameter_rows,
    candidate_loans::NUMBER(38,0) AS candidate_loans,
    no_candidate_loans::NUMBER(38,0) AS no_candidate_loans,
    loan_document_pairs::NUMBER(38,0) AS loan_document_pairs,
    candidate_document_rows::NUMBER(38,0) AS candidate_document_rows,
    whole_plane_reconciles::BOOLEAN AS whole_plane_reconciles,
    shard_candidate_reconciles::BOOLEAN AS shard_candidate_reconciles,
    loan_key::TEXT AS loan_key,
    assigned_shard_index::NUMBER(38,0) AS assigned_shard_index,
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
    recorded_borough::NUMBER(38,0) AS recorded_borough,
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
    acris_party_source_record_ids AS acris_party_source_record_ids,
    acris_party_raw_csv_sha256s AS acris_party_raw_csv_sha256s,
    acris_party_filenames AS acris_party_filenames
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_STAGE2_CANDIDATE_QUERY_ID__'))
),
stage2_rows_or_empty AS (
  SELECT s.*
  FROM (SELECT 1 AS anchor) a
  LEFT JOIN stage2_rows s
    ON TRUE
),
stage2_stats AS (
  SELECT
    COUNT(*) AS stage2_result_rows,
    COUNT_IF(COALESCE(stage2_row_contract, '')
      <> (SELECT expected_stage2_row_contract FROM residual_params))
      AS stage2_contract_mismatch_rows,
    COUNT_IF(COALESCE(selected_truth_plane, '')
      <> (SELECT selected_truth_plane FROM residual_params))
      AS stage2_truth_plane_mismatch_rows,
    COUNT_IF(shard_count IS NULL
      OR shard_count <> (SELECT shard_count FROM residual_params))
      AS stage2_shard_count_mismatch_rows,
    COUNT_IF(shard_index IS NULL
      OR shard_index <> (SELECT shard_index FROM residual_params))
      AS stage2_shard_index_mismatch_rows,
    COUNT_IF(COALESCE(stage2_guard_status, '') <> 'ok')
      AS stage2_guard_refused_rows,
    COUNT_IF(COALESCE(stage2_row_kind, '') NOT IN (
      'candidate',
      'no_candidate',
      'guard_failure'
    ))
      AS stage2_unexpected_row_kind_rows,
    COUNT_IF(NOT COALESCE(whole_plane_reconciles, FALSE))
      AS stage2_whole_plane_reconcile_fail_rows,
    COUNT_IF(NOT COALESCE(shard_candidate_reconciles, FALSE))
      AS stage2_shard_reconcile_fail_rows,
    COUNT_IF(COALESCE(bridge_build_id, '')
      <> (SELECT bridge_build_id FROM residual_params))
      AS stage2_bridge_build_mismatch_rows,
    COUNT_IF(acris_release_dt IS NULL
      OR acris_release_dt <> (SELECT acris_release_dt FROM residual_params))
      AS stage2_acris_release_mismatch_rows,
    COUNT_IF(stage2_row_kind = 'candidate' AND document_id IS NULL)
      AS stage2_candidate_key_missing_rows,
    COUNT_IF(stage2_row_kind = 'candidate'
      AND (filed_boroughs IS NULL OR ARRAY_SIZE(filed_boroughs) = 0))
      AS stage2_candidate_missing_filed_boroughs_rows
  FROM stage2_rows
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT
      'bridge_build_id_sentinel_unsubstituted' AS failure_reason,
      (SELECT bridge_build_id FROM residual_params)
        = (SELECT bridge_build_id_unbound_marker
           FROM residual_sentinel_markers) AS failed
    UNION ALL
    SELECT
      'stage2_candidate_query_id_sentinel_unsubstituted' AS failure_reason,
      (SELECT stage2_candidate_query_id FROM residual_params)
        = (SELECT stage2_candidate_query_id_unbound_marker
           FROM residual_sentinel_markers) AS failed
    UNION ALL
    SELECT
      'truth_plane_sentinel_unsubstituted',
      (SELECT selected_truth_plane FROM residual_params)
        = (SELECT truth_plane_unbound_marker FROM residual_sentinel_markers)
    UNION ALL
    SELECT
      'shard_count_sentinel_unsubstituted',
      (SELECT shard_count_literal FROM residual_params)
        = (SELECT shard_count_unbound_marker FROM residual_sentinel_markers)
    UNION ALL
    SELECT
      'shard_index_sentinel_unsubstituted',
      (SELECT shard_index_literal FROM residual_params)
        = (SELECT shard_index_unbound_marker FROM residual_sentinel_markers)
    UNION ALL
    SELECT
      'invalid_selected_truth_plane',
      (SELECT selected_truth_plane FROM residual_params) NOT IN (
        'non_round_amount_date_legal_borough',
        'round_exact_lender_party'
      )
    UNION ALL
    SELECT
      'invalid_shard_count',
      (SELECT shard_count FROM residual_params) IS NULL
        OR (SELECT shard_count FROM residual_params) < 1
        OR (SELECT shard_count FROM residual_params)
          > (SELECT max_shard_count FROM residual_params)
        OR (SELECT shard_count FROM residual_params)
          <> FLOOR((SELECT shard_count FROM residual_params))
    UNION ALL
    SELECT
      'invalid_shard_index',
      (SELECT shard_index FROM residual_params) IS NULL
        OR (SELECT shard_index FROM residual_params) < 0
        OR (SELECT shard_index FROM residual_params)
          >= COALESCE((SELECT shard_count FROM residual_params), 0)
        OR (SELECT shard_index FROM residual_params)
          <> FLOOR((SELECT shard_index FROM residual_params))
    UNION ALL
    SELECT 'stage2_result_scan_empty',
      (SELECT stage2_result_rows FROM stage2_stats) = 0
    UNION ALL
    SELECT 'stage2_result_scan_exceeds_bound',
      (SELECT stage2_result_rows FROM stage2_stats)
        > (SELECT max_stage2_rows FROM residual_params)
    UNION ALL
    SELECT 'stage2_row_contract_mismatch',
      (SELECT stage2_contract_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_truth_plane_mismatch',
      (SELECT stage2_truth_plane_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_shard_count_mismatch',
      (SELECT stage2_shard_count_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_shard_index_mismatch',
      (SELECT stage2_shard_index_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_guard_refused',
      (SELECT stage2_guard_refused_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_unexpected_row_kind',
      (SELECT stage2_unexpected_row_kind_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_whole_plane_denominator_does_not_reconcile',
      (SELECT stage2_whole_plane_reconcile_fail_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_shard_denominator_does_not_reconcile',
      (SELECT stage2_shard_reconcile_fail_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_bridge_build_id_mismatch',
      (SELECT stage2_bridge_build_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_acris_release_dt_mismatch',
      (SELECT stage2_acris_release_mismatch_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_candidate_key_missing',
      (SELECT stage2_candidate_key_missing_rows FROM stage2_stats) <> 0
    UNION ALL
    SELECT 'stage2_candidate_missing_filed_boroughs',
      (SELECT stage2_candidate_missing_filed_boroughs_rows FROM stage2_stats)
        <> 0
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS stage3_guard_status,
    MIN(failure_reason) AS stage3_refusal_reason
  FROM guard_failures
),
row_emission_modes AS (
  SELECT
    column1::TEXT AS emission_mode,
    column2::TEXT AS required_stage2_row_kind,
    column3::BOOLEAN AS probes_legal
  FROM VALUES
    ('candidate', 'candidate', FALSE),
    ('no_candidate', 'no_candidate', FALSE),
    ('guard_failure', 'guard_failure', FALSE),
    ('legal_probe', 'candidate', TRUE)
),
candidate_loan_document_borough_keys AS (
  SELECT DISTINCT
    s.loan_key,
    s.document_id,
    filed.value::NUMBER(38,0) AS filed_borough
  FROM stage2_rows s,
    LATERAL FLATTEN(input => s.filed_boroughs) filed
  WHERE (SELECT stage3_guard_status FROM guard_summary) = 'ok'
    AND s.stage2_row_kind = 'candidate'
    AND s.stage2_guard_status = 'ok'
    AND s.loan_key IS NOT NULL
    AND s.document_id IS NOT NULL
    AND filed.value::NUMBER(38,0) IN (1, 2, 3, 4, 5)
),
acris_legals AS (
  SELECT
    k.loan_key AS legal_loan_key,
    k.filed_borough,
    l.release_dt,
    l.source_row_number::NUMBER(38,0) AS legal_source_row_number,
    l.document_id::TEXT AS legal_document_id,
    l.record_type::TEXT AS legal_record_type,
    l.legal_borough::NUMBER(38,0) AS legal_borough,
    l.block::NUMBER(38,0) AS legal_block,
    l.lot::NUMBER(38,0) AS legal_lot,
    l.bbl::TEXT AS legal_bbl_raw,
    COALESCE(
      REGEXP_REPLACE(TO_VARCHAR(l.bbl), '[.]0$', ''),
      TO_VARCHAR(l.legal_borough::NUMBER(38,0))
        || LPAD(TO_VARCHAR(l.block::NUMBER(38,0)), 5, '0')
        || LPAD(TO_VARCHAR(l.lot::NUMBER(38,0)), 4, '0')
    ) AS legal_bbl,
    TO_VARCHAR(l.legal_borough::NUMBER(38,0))
      || LPAD(TO_VARCHAR(l.block::NUMBER(38,0)), 5, '0') AS legal_block_key,
    IFF(TRY_TO_NUMBER(l.lot) BETWEEN 1001 AND 6999, TRUE, FALSE)
      AS legal_is_condo_unit_lot,
    l.good_through_date AS legal_good_through_date,
    l.raw_csv_sha256::TEXT AS legal_raw_csv_sha256,
    l.filename::TEXT AS legal_filename,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS:'
      || TO_VARCHAR(l.release_dt)
      || ':'
      || TO_VARCHAR(l.source_row_number::NUMBER(38,0))
      || ':'
      || l.document_id::TEXT
      || ':'
      || TO_VARCHAR(l.legal_borough::NUMBER(38,0))
      || ':'
      || TO_VARCHAR(l.block::NUMBER(38,0))
      || ':'
      || TO_VARCHAR(l.lot::NUMBER(38,0)) AS acris_legal_source_record_id
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS l
  JOIN candidate_loan_document_borough_keys k
    ON l.document_id = k.document_id
   AND l.legal_borough = k.filed_borough
  WHERE l.release_dt = (SELECT acris_release_dt FROM residual_params)
    AND l.legal_borough IN (1, 2, 3, 4, 5)
    AND l.block IS NOT NULL
    AND l.lot IS NOT NULL
)
SELECT
  (SELECT output_row_contract FROM residual_params) AS row_contract,
  CASE
    WHEN m.emission_mode = 'legal_probe' AND l.legal_bbl IS NULL
      THEN 'no_legal'
    WHEN m.emission_mode = 'legal_probe' THEN 'legal'
    ELSE m.emission_mode
  END AS row_kind,
  IFF(
    g.stage3_guard_status = 'ok'
      AND COALESCE(s.stage2_guard_status, '') = 'ok',
    'ok',
    'refused'
  ) AS guard_status,
  COALESCE(
    g.stage3_refusal_reason,
    IFF(s.stage2_guard_status = 'ok', NULL, s.stage2_guard_failure_reason)
  ) AS refusal_reason,
  (SELECT stage2_candidate_query_id FROM residual_params)
    AS stage2_candidate_query_id,
  s.upstream_loan_parameter_query_id,
  s.stage2_row_contract,
  s.stage2_row_kind,
  s.stage2_guard_status,
  s.stage2_guard_failure_reason,
  s.selected_truth_plane,
  s.shard_count,
  s.shard_index,
  s.assigned_shard_index,
  s.shard_partition_expression,
  s.bridge_build_id,
  TO_VARCHAR(s.acris_release_dt) AS acris_release_dt,
  p.mappluto_release,
  TO_VARCHAR(p.mappluto_release_dt) AS mappluto_release_dt,
  p.mappluto_variant,
  p.mappluto_geometry_plane,
  s.property_state,
  s.collateral_scope,
  s.amount_cents_quantization,
  s.round_amount_lattice_cents,
  s.max_recording_offset_days,
  s.whole_plane_eligible_loans,
  s.whole_plane_loan_parameter_rows,
  s.shard_loan_parameter_rows,
  s.candidate_loans,
  s.no_candidate_loans,
  s.loan_document_pairs,
  s.candidate_document_rows,
  s.whole_plane_reconciles,
  s.shard_candidate_reconciles,
  s.loan_key,
  s.property_keys,
  s.association_plane,
  s.amount_cents,
  TO_VARCHAR(s.originationdate) AS originationdate,
  s.originatorname,
  s.originator_match_text,
  s.filed_counties,
  s.filed_boroughs,
  s.filed_county_borough_edges,
  s.distinct_counts,
  s.diagnostic_county_fips,
  s.document_id,
  s.recorded_borough,
  IFF(m.probes_legal, k.filed_borough, NULL)::NUMBER(38,0) AS filed_borough,
  s.doc_type,
  s.crfn,
  TO_VARCHAR(s.document_date) AS document_date,
  TO_VARCHAR(s.recorded_date) AS recorded_date,
  s.recording_offset_days,
  s.lender_match_text,
  s.lender_party_type,
  IFF(m.probes_legal, l.legal_document_id, NULL)::TEXT AS legal_document_id,
  IFF(m.probes_legal, l.legal_source_row_number, NULL)::NUMBER(38,0)
    AS legal_source_row_number,
  IFF(m.probes_legal, l.legal_record_type, NULL)::TEXT AS legal_record_type,
  IFF(m.probes_legal, l.legal_borough, NULL)::NUMBER(38,0) AS legal_borough,
  IFF(m.probes_legal, l.legal_block, NULL)::NUMBER(38,0) AS legal_block,
  IFF(m.probes_legal, l.legal_lot, NULL)::NUMBER(38,0) AS legal_lot,
  IFF(m.probes_legal, l.legal_bbl_raw, NULL)::TEXT AS legal_bbl_raw,
  IFF(m.probes_legal, l.legal_bbl, NULL)::TEXT AS legal_bbl,
  IFF(m.probes_legal, l.legal_block_key, NULL)::TEXT AS legal_block_key,
  IFF(m.probes_legal, l.legal_is_condo_unit_lot, NULL)::BOOLEAN
    AS legal_is_condo_unit_lot,
  IFF(m.probes_legal, TO_VARCHAR(l.legal_good_through_date), NULL)::TEXT
    AS legal_good_through_date,
  IFF(m.probes_legal, l.legal_raw_csv_sha256, NULL)::TEXT
    AS legal_raw_csv_sha256,
  IFF(m.probes_legal, l.legal_filename, NULL)::TEXT AS legal_filename,
  IFF(m.probes_legal, REGEXP_REPLACE(TO_VARCHAR(mp.bbl), '[.]0$', ''), NULL)::TEXT
    AS mappluto_bbl,
  IFF(m.probes_legal, mp.source_row_number::NUMBER(38,0), NULL)::NUMBER(38,0)
    AS mappluto_source_row_number,
  IFF(m.probes_legal, mp.geometry_evidence_contract_version::TEXT, NULL)::TEXT
    AS mappluto_geometry_evidence_contract_version,
  IFF(m.probes_legal, mp.transform_execution_id::TEXT, NULL)::TEXT
    AS mappluto_transform_execution_id,
  IFF(m.probes_legal, mp.source_filename::TEXT, NULL)::TEXT
    AS mappluto_source_filename,
  IFF(m.probes_legal, mp.source_geom_wkb_sha256::TEXT, NULL)::TEXT
    AS mappluto_source_geom_wkb_sha256,
  IFF(m.probes_legal, mp.geom_wgs84_sha256::TEXT, NULL)::TEXT
    AS mappluto_geom_wgs84_sha256,
  IFF(m.probes_legal, mp.source_geom_crs::TEXT, NULL)::TEXT
    AS mappluto_source_geom_crs,
  IFF(m.probes_legal, mp.source_geom_srid::NUMBER(38,0), NULL)::NUMBER(38,0)
    AS mappluto_source_geom_srid,
  IFF(m.probes_legal, mp.bbl IS NOT NULL, NULL)::BOOLEAN
    AS mappluto_bbl_present,
  IFF(m.probes_legal, mp.geom_wgs84_sha256 IS NOT NULL, NULL)::BOOLEAN
    AS mappluto_geom_present,
  CASE
    WHEN NOT m.probes_legal THEN 'not_applicable'
    WHEN l.legal_bbl IS NULL THEN 'no_legal_bbl'
    WHEN mp.bbl IS NULL THEN 'legal_bbl_missing_from_mappluto_release'
    ELSE 'legal_bbl_present_in_mappluto_release'
  END AS bbl_release_sensitivity,
  CASE
    WHEN NOT m.probes_legal THEN 'not_applicable'
    WHEN l.legal_bbl IS NULL THEN 'no_legal'
    ELSE 'legal'
  END AS legal_status,
  s.bridge_source_record_ids,
  s.acris_master_source_record_id,
  s.acris_master_raw_csv_sha256,
  s.acris_master_filename,
  s.acris_party_source_record_ids,
  s.acris_party_raw_csv_sha256s,
  s.acris_party_filenames,
  IFF(m.probes_legal, l.acris_legal_source_record_id, NULL)::TEXT
    AS acris_legal_source_record_id,
  IFF(
    m.probes_legal AND mp.bbl IS NOT NULL,
    'EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT:'
      || p.mappluto_release
      || ':'
      || TO_VARCHAR(p.mappluto_release_dt)
      || ':'
      || COALESCE(TO_VARCHAR(mp."VARIANT"), '')
      || ':'
      || TO_VARCHAR(mp.source_row_number::NUMBER(38,0))
      || ':'
      || REGEXP_REPLACE(TO_VARCHAR(mp.bbl), '[.]0$', ''),
    NULL
  ) AS mappluto_source_record_id
FROM stage2_rows_or_empty s
JOIN row_emission_modes m
  ON m.required_stage2_row_kind = COALESCE(s.stage2_row_kind, 'guard_failure')
CROSS JOIN guard_summary g
LEFT JOIN mappluto_release_pins p
  ON m.probes_legal
LEFT JOIN candidate_loan_document_borough_keys k
  ON m.probes_legal
 AND k.loan_key = s.loan_key
 AND k.document_id = s.document_id
LEFT JOIN acris_legals l
  ON m.probes_legal
 AND l.legal_loan_key = s.loan_key
 AND l.legal_document_id = s.document_id
 AND l.legal_borough = k.filed_borough
LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT mp
  ON m.probes_legal
 AND l.legal_bbl IS NOT NULL
 AND mp.release = p.mappluto_release
 AND mp.release_dt = p.mappluto_release_dt
 AND TO_VARCHAR(mp."VARIANT") = p.mappluto_variant
 AND REGEXP_REPLACE(TO_VARCHAR(mp.bbl), '[.]0$', '') = l.legal_bbl
ORDER BY
  row_kind,
  s.loan_key,
  s.document_id,
  legal_bbl,
  mappluto_release;
