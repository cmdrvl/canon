-- Appendix H.7 Stage 2 flat ACRIS MASTER/PARTY candidate shard SQL.
--
-- This is the second of three acquisition-side single-SELECT stages for
-- bd-7bcp. It consumes the Stage-1 bridge-only loan-parameter payload through
-- RESULT_SCAN, filters one deterministic loan-key shard, and emits flat rows:
-- one row per shard loan/document/recorded-borough candidate plus one
-- no-candidate row for each shard loan without a MASTER/PARTY candidate. It
-- does not join ACRIS LEGALS or MapPLUTO, does not emit nested candidate
-- arrays, and cannot produce canon_geo_h7_population_rows.v0.
--
-- Coordinator sequence:
--   1. Run h7_multi_parcel_population.sql once per selected plane and preserve
--      its Snowflake query id.
--   2. Byte-substitute only the operational placeholder literals in
--      candidate_params and RESULT_SCAN:
--        '__BD7BCP_H7_LOAN_PARAMETER_QUERY_ID__'
--        '__BD7BCP_H7_TRUTH_PLANE__'
--        '__BD7BCP_H7_SHARD_COUNT__'
--        '__BD7BCP_H7_SHARD_INDEX__'
--      Do not rewrite candidate_sentinel_markers; those split marker
--      expressions are the immutable unbound guards.
--   3. Intended shard plans after the physical candidate access path is
--      repaired: 16 shards for 'non_round_amount_date_legal_borough' and 64
--      shards for 'round_exact_lender_party'. Do not fan these out against the
--      current external-table shape: the 0/16 and 0/64 controls below both hit
--      the same client-cancellation boundary.
--   4. Run h7_multi_parcel_legal_residual.sql once for each successful
--      Stage-2 shard query id with the same plane, shard_count, and
--      shard_index.
--
-- Discarded/cancelled attempts retained as context only:
-- * 01c6beec-0821-9afc-006c-c703088ce266, 45s: combined shape cancelled.
-- * 01c6bef3-0821-9afc-006c-c703088ce26a, 45.05s: monolithic non-round
--   candidate/control shape cancelled.
-- * 01c6befd-0821-9afc-006c-c703088ce26e, 6.966s: true Stage-1 non-round run
--   succeeded with guard ok and 653 exported rows. 01c6befe was only the
--   self-matching query-history control.
-- * 01c6bf06-0821-9afc-006c-c703088ce28e, 45.084s: alias-fixed unsharded
--   Stage 2 timed out; rendered SHA-256
--   7b56597985857254b5e940686a287f71fe2faa5a9fead4bfb6b688c3c17367bb.
-- * 01c6bf19-0821-a6c8-006c-c703088d02f2, 45.061s: nested shard 0/16
--   non-round Stage 2 timed out; source SHA
--   511b19fb5a706de5833a1b282a4efeff194e3bb5700405efde970173787a3d04,
--   normalized digest 476a6cd8ec3722de4c773a1e92693654971dd450bf71ad9311af86632185c33e.
-- * 01c6bf36-0821-a0dc-006c-c703088cf4ca, 45.129s: flat shard 0/16
--   non-round Stage 2 timed out after the nested-FLATTEN compile defect was
--   removed; the retained local rendered-source digest begins dfe5e330.
-- * 01c6bf38-0821-a0dc-006c-c703088cf4d2, 45.052s: flat shard 0/64
--   non-round Stage 2 also timed out. The smaller logical shard did not reduce
--   the 954,601-byte external MASTER scan, so retries are stopped pending a
--   release-pinned candidate fact/index or a retrievable long-read MCP path.
--
-- This row stream has no source hashes and no legal truth. Downstream
-- materialization must bind the executed SQL text/hash/query id and attach
-- preserved, syntactically validated source hashes.

WITH
candidate_params AS (
  SELECT
    '__BD7BCP_H7_LOAN_PARAMETER_QUERY_ID__'::TEXT AS loan_parameter_query_id,
    '__BD7BCP_H7_TRUTH_PLANE__'::TEXT AS selected_truth_plane,
    '__BD7BCP_H7_SHARD_COUNT__'::TEXT AS shard_count_literal,
    '__BD7BCP_H7_SHARD_INDEX__'::TEXT AS shard_index_literal,
    TRY_TO_NUMBER('__BD7BCP_H7_SHARD_COUNT__')::NUMBER(38,0) AS shard_count,
    TRY_TO_NUMBER('__BD7BCP_H7_SHARD_INDEX__')::NUMBER(38,0) AS shard_index,
    'h7_bridge_loan_parameters.v0'::TEXT AS expected_payload_kind,
    'h7_bridge_loan_parameters.v0'::TEXT AS expected_payload_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT AS amount_cents_quantization,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days,
    3000::NUMBER(38,0) AS max_loan_parameter_rows,
    64::NUMBER(38,0) AS max_shard_loan_parameter_rows,
    10000::NUMBER(38,0) AS max_candidate_borough_rows,
    256::NUMBER(38,0) AS max_shard_count,
    'MOD(ABS(MD5_NUMBER_LOWER64(TO_VARCHAR(loan_key))), shard_count)'::TEXT
      AS shard_partition_expression
),
candidate_sentinel_markers AS (
  SELECT
    ('__BD7BCP_H7_' || 'LOAN_PARAMETER_QUERY_ID__')::TEXT
      AS loan_parameter_query_id_unbound_marker,
    ('__BD7BCP_H7_' || 'TRUTH_PLANE__')::TEXT
      AS truth_plane_unbound_marker,
    ('__BD7BCP_H7_' || 'SHARD_COUNT__')::TEXT
      AS shard_count_unbound_marker,
    ('__BD7BCP_H7_' || 'SHARD_INDEX__')::TEXT
      AS shard_index_unbound_marker
),
mortgage_doc_types AS (
  SELECT column1::TEXT AS doc_type
  FROM VALUES ('MTGE'), ('M&CON'), ('CMTG'), ('SMTG'), ('MMTG'), ('SPRD')
),
lender_party_roles AS (
  SELECT * FROM VALUES
    ('CMTG', '2'),
    ('M&CON', '2'),
    ('MMTG', '1'),
    ('MTGE', '2'),
    ('SMTG', '2'),
    ('SPRD', '2')
  AS r(doc_type, lender_party_type)
),
scanned_stage1 AS (
  SELECT h7_loan_parameter_payload::VARIANT AS payload
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_LOAN_PARAMETER_QUERY_ID__'))
),
stage1_payload AS (
  SELECT payload
  FROM scanned_stage1
  LIMIT 1
),
selected_sections AS (
  SELECT section.value AS section_payload
  FROM stage1_payload p,
    LATERAL FLATTEN(input => p.payload:loan_parameter_array_export) section
  WHERE section.value:truth_plane::TEXT =
    (SELECT selected_truth_plane FROM candidate_params)
),
section_stats AS (
  SELECT
    (SELECT COUNT(*) FROM scanned_stage1) AS result_scan_rows,
    COALESCE((
      SELECT ARRAY_SIZE(payload:loan_parameter_array_export)
      FROM stage1_payload
    ), 0) AS total_section_count,
    (SELECT COUNT(*) FROM selected_sections) AS selected_section_count,
    COALESCE((
      SELECT MAX(section_payload:loan_parameter_rows::NUMBER(38,0))
      FROM selected_sections
    ), 0) AS payload_loan_parameter_rows
),
whole_plane_control AS (
  SELECT
    COALESCE(MAX(section.value:eligible_loans::NUMBER(38,0)), 0)
      AS whole_plane_eligible_loans
  FROM stage1_payload p,
    LATERAL FLATTEN(input => p.payload:controls:raw_property_state_ny_universe) section
  WHERE section.value:truth_plane::TEXT =
    (SELECT selected_truth_plane FROM candidate_params)
),
loan_parameters AS (
  SELECT DISTINCT
    entry.value:loan_key::TEXT AS loan_key,
    entry.value:truth_plane::TEXT AS truth_plane,
    entry.value:property_state::TEXT AS property_state,
    entry.value:collateral_scope::TEXT AS collateral_scope,
    entry.value:property_keys::NUMBER(38,0) AS property_keys,
    entry.value:association_plane::TEXT AS association_plane,
    entry.value:amount_cents::NUMBER(38,0) AS amount_cents,
    TRY_TO_DATE(entry.value:originationdate::TEXT) AS originationdate,
    entry.value:originatorname::TEXT AS originatorname,
    entry.value:originator_match_text::TEXT AS originator_match_text,
    entry.value:filed_counties AS filed_counties,
    entry.value:filed_boroughs AS filed_boroughs,
    entry.value:filed_county_borough_edges AS filed_county_borough_edges,
    entry.value:distinct_counts AS distinct_counts,
    entry.value:diagnostic_county_fips AS diagnostic_county_fips,
    entry.value:bridge_source_record_ids AS bridge_source_record_ids
  FROM selected_sections s,
    LATERAL FLATTEN(input => s.section_payload:loan_parameter_rows_payload) entry
),
loan_parameters_with_shard AS (
  SELECT
    *,
    MOD(
      ABS(MD5_NUMBER_LOWER64(TO_VARCHAR(loan_key))),
      NULLIF((SELECT shard_count FROM candidate_params), 0)
    )::NUMBER(38,0) AS assigned_shard_index
  FROM loan_parameters
),
sharded_loan_parameters AS (
  SELECT *
  FROM loan_parameters_with_shard
  WHERE assigned_shard_index = (SELECT shard_index FROM candidate_params)
),
loan_parameter_validity AS (
  SELECT
    COUNT(*) AS loan_parameter_rows,
    COUNT_IF(loan_key IS NULL OR loan_key = '') AS missing_loan_key_rows,
    COUNT_IF(truth_plane <> (SELECT selected_truth_plane FROM candidate_params)
      OR truth_plane IS NULL) AS truth_plane_mismatch_rows,
    COUNT_IF(property_state <> (SELECT property_state FROM candidate_params)
      OR property_state IS NULL) AS property_state_mismatch_rows,
    COUNT_IF(collateral_scope <> (SELECT collateral_scope FROM candidate_params)
      OR collateral_scope IS NULL) AS collateral_scope_mismatch_rows,
    COUNT_IF(amount_cents IS NULL OR amount_cents = 0) AS invalid_amount_rows,
    COUNT_IF(originationdate IS NULL) AS invalid_originationdate_rows,
    COUNT_IF(filed_boroughs IS NULL OR ARRAY_SIZE(filed_boroughs) = 0)
      AS missing_filed_boroughs_rows,
    COUNT_IF(filed_county_borough_edges IS NULL
      OR ARRAY_SIZE(filed_county_borough_edges) = 0)
      AS missing_filed_edges_rows
  FROM loan_parameters
),
shard_counts AS (
  SELECT COUNT(*) AS shard_loan_parameter_rows
  FROM sharded_loan_parameters
),
master_candidates_non_round AS (
  SELECT DISTINCT
    l.loan_key,
    'non_round_amount_date_legal_borough'::TEXT AS truth_plane,
    l.assigned_shard_index,
    l.property_state,
    l.collateral_scope,
    l.property_keys,
    l.association_plane,
    l.amount_cents,
    l.originationdate,
    l.originatorname,
    l.originator_match_text,
    l.filed_counties,
    l.filed_boroughs,
    l.filed_county_borough_edges,
    l.distinct_counts,
    l.diagnostic_county_fips,
    l.bridge_source_record_ids,
    m.document_id::TEXT AS document_id,
    m.recorded_borough::NUMBER(38,0) AS recorded_borough,
    UPPER(TRIM(m.doc_type))::TEXT AS doc_type,
    m.crfn::TEXT AS crfn,
    CAST(m.document_date AS DATE) AS document_date,
    CAST(m.recorded_datetime AS DATE) AS recorded_date,
    DATEDIFF(day, l.originationdate, CAST(m.recorded_datetime AS DATE))
      AS recording_offset_days,
    'EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT:'
      || TO_VARCHAR((SELECT acris_release_dt FROM candidate_params))
      || ':'
      || m.document_id::TEXT AS acris_master_source_record_id,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::TEXT AS acris_party_source_record_id
  FROM sharded_loan_parameters l
  JOIN candidate_params selected
    ON selected.selected_truth_plane = 'non_round_amount_date_legal_borough'
  JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT m
    ON m.release_dt = (SELECT acris_release_dt FROM candidate_params)
   AND ROUND(m.document_amt * 100, 0)::NUMBER(38,0) = l.amount_cents
   AND CAST(m.recorded_datetime AS DATE) BETWEEN l.originationdate
     AND DATEADD(day, (SELECT max_recording_offset_days FROM candidate_params),
       l.originationdate)
  JOIN mortgage_doc_types dt
    ON UPPER(TRIM(m.doc_type)) = dt.doc_type
  WHERE l.truth_plane = 'non_round_amount_date_legal_borough'
    AND ARRAY_CONTAINS(m.recorded_borough::VARIANT, l.filed_boroughs)
),
master_candidates_round AS (
  SELECT DISTINCT
    l.loan_key,
    'round_exact_lender_party'::TEXT AS truth_plane,
    l.assigned_shard_index,
    l.property_state,
    l.collateral_scope,
    l.property_keys,
    l.association_plane,
    l.amount_cents,
    l.originationdate,
    l.originatorname,
    l.originator_match_text,
    l.filed_counties,
    l.filed_boroughs,
    l.filed_county_borough_edges,
    l.distinct_counts,
    l.diagnostic_county_fips,
    l.bridge_source_record_ids,
    m.document_id::TEXT AS document_id,
    m.recorded_borough::NUMBER(38,0) AS recorded_borough,
    UPPER(TRIM(m.doc_type))::TEXT AS doc_type,
    m.crfn::TEXT AS crfn,
    CAST(m.document_date AS DATE) AS document_date,
    CAST(m.recorded_datetime AS DATE) AS recorded_date,
    DATEDIFF(day, l.originationdate, CAST(m.recorded_datetime AS DATE))
      AS recording_offset_days,
    'EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT:'
      || TO_VARCHAR((SELECT acris_release_dt FROM candidate_params))
      || ':'
      || m.document_id::TEXT AS acris_master_source_record_id,
    TRIM(REGEXP_REPLACE(UPPER(party.name), '[^A-Z0-9 ]', ' '))
      AS lender_match_text,
    party.party_type::TEXT AS lender_party_type,
    'EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT:'
      || TO_VARCHAR((SELECT acris_release_dt FROM candidate_params))
      || ':'
      || m.document_id::TEXT
      || ':'
      || party.party_type::TEXT
      || ':'
      || TRIM(REGEXP_REPLACE(UPPER(party.name), '[^A-Z0-9 ]', ' '))
      AS acris_party_source_record_id
  FROM sharded_loan_parameters l
  JOIN candidate_params selected
    ON selected.selected_truth_plane = 'round_exact_lender_party'
  JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT m
    ON m.release_dt = (SELECT acris_release_dt FROM candidate_params)
   AND ROUND(m.document_amt * 100, 0)::NUMBER(38,0) = l.amount_cents
   AND CAST(m.recorded_datetime AS DATE) BETWEEN l.originationdate
     AND DATEADD(day, (SELECT max_recording_offset_days FROM candidate_params),
       l.originationdate)
  JOIN mortgage_doc_types dt
    ON UPPER(TRIM(m.doc_type)) = dt.doc_type
  JOIN lender_party_roles role
    ON UPPER(TRIM(m.doc_type)) = role.doc_type
  JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT party
    ON party.release_dt = m.release_dt
   AND party.document_id = m.document_id
   AND party.party_type::TEXT = role.lender_party_type
   AND TRIM(REGEXP_REPLACE(UPPER(party.name), '[^A-Z0-9 ]', ' '))
      = l.originator_match_text
  WHERE l.truth_plane = 'round_exact_lender_party'
    AND l.originator_match_text IS NOT NULL
    AND ARRAY_CONTAINS(m.recorded_borough::VARIANT, l.filed_boroughs)
),
master_candidates AS (
  SELECT * FROM master_candidates_non_round
  UNION ALL
  SELECT * FROM master_candidates_round
),
candidate_counts AS (
  SELECT
    COUNT(*) AS candidate_borough_rows,
    COUNT(DISTINCT loan_key) AS candidate_loans,
    COUNT(DISTINCT loan_key || '|' || document_id) AS loan_document_pairs
  FROM master_candidates
),
no_candidate_loans AS (
  SELECT l.*
  FROM sharded_loan_parameters l
  LEFT JOIN (
    SELECT DISTINCT loan_key
    FROM master_candidates
  ) c
    ON c.loan_key = l.loan_key
  WHERE c.loan_key IS NULL
),
no_candidate_counts AS (
  SELECT COUNT(*) AS no_candidate_loans
  FROM no_candidate_loans
),
denominator AS (
  SELECT
    p.selected_truth_plane AS truth_plane,
    p.shard_count,
    p.shard_index,
    p.shard_partition_expression,
    COALESCE(w.whole_plane_eligible_loans, 0) AS whole_plane_eligible_loans,
    COALESCE(v.loan_parameter_rows, 0) AS whole_plane_loan_parameter_rows,
    COALESCE(s.shard_loan_parameter_rows, 0) AS shard_loan_parameter_rows,
    COALESCE(c.candidate_loans, 0) AS candidate_loans,
    COALESCE(c.loan_document_pairs, 0) AS loan_document_pairs,
    COALESCE(c.candidate_borough_rows, 0) AS candidate_borough_rows,
    COALESCE(n.no_candidate_loans, 0) AS no_candidate_loans,
    COALESCE(w.whole_plane_eligible_loans, 0) = COALESCE(v.loan_parameter_rows, 0)
      AS whole_plane_reconciles,
    COALESCE(s.shard_loan_parameter_rows, 0) = COALESCE(c.candidate_loans, 0)
      + COALESCE(n.no_candidate_loans, 0) AS shard_candidate_reconciles
  FROM candidate_params p
  CROSS JOIN whole_plane_control w
  CROSS JOIN loan_parameter_validity v
  CROSS JOIN shard_counts s
  CROSS JOIN candidate_counts c
  CROSS JOIN no_candidate_counts n
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT
      'result_scan_row_count_not_one' AS failure_reason,
      (SELECT result_scan_rows FROM section_stats) <> 1 AS failed
    UNION ALL
    SELECT
      'loan_parameter_query_id_sentinel_unsubstituted',
      (SELECT loan_parameter_query_id FROM candidate_params)
        = (SELECT loan_parameter_query_id_unbound_marker
           FROM candidate_sentinel_markers)
    UNION ALL
    SELECT
      'truth_plane_sentinel_unsubstituted',
      (SELECT selected_truth_plane FROM candidate_params)
        = (SELECT truth_plane_unbound_marker FROM candidate_sentinel_markers)
    UNION ALL
    SELECT
      'shard_count_sentinel_unsubstituted',
      (SELECT shard_count_literal FROM candidate_params)
        = (SELECT shard_count_unbound_marker FROM candidate_sentinel_markers)
    UNION ALL
    SELECT
      'shard_index_sentinel_unsubstituted',
      (SELECT shard_index_literal FROM candidate_params)
        = (SELECT shard_index_unbound_marker FROM candidate_sentinel_markers)
    UNION ALL
    SELECT
      'invalid_selected_truth_plane',
      (SELECT selected_truth_plane FROM candidate_params) NOT IN (
        'non_round_amount_date_legal_borough',
        'round_exact_lender_party'
      )
    UNION ALL
    SELECT
      'invalid_shard_count',
      (SELECT shard_count FROM candidate_params) IS NULL
        OR (SELECT shard_count FROM candidate_params) < 1
        OR (SELECT shard_count FROM candidate_params)
          > (SELECT max_shard_count FROM candidate_params)
        OR (SELECT shard_count FROM candidate_params)
          <> FLOOR((SELECT shard_count FROM candidate_params))
    UNION ALL
    SELECT
      'invalid_shard_index',
      (SELECT shard_index FROM candidate_params) IS NULL
        OR (SELECT shard_index FROM candidate_params) < 0
        OR (SELECT shard_index FROM candidate_params)
          >= COALESCE((SELECT shard_count FROM candidate_params), 0)
        OR (SELECT shard_index FROM candidate_params)
          <> FLOOR((SELECT shard_index FROM candidate_params))
    UNION ALL
    SELECT
      'payload_kind_mismatch',
      COALESCE((SELECT payload:payload_kind::TEXT FROM stage1_payload), '')
        <> (SELECT expected_payload_kind FROM candidate_params)
    UNION ALL
    SELECT
      'payload_contract_mismatch',
      COALESCE((SELECT payload:payload_contract::TEXT FROM stage1_payload), '')
        <> (SELECT expected_payload_contract FROM candidate_params)
    UNION ALL
    SELECT
      'payload_guard_not_ok',
      COALESCE((SELECT payload:loan_parameter_guard_status::TEXT
                FROM stage1_payload), '') <> 'ok'
    UNION ALL
    SELECT
      'payload_selected_truth_plane_mismatch',
      COALESCE((SELECT payload:selected_truth_plane::TEXT FROM stage1_payload), '')
        <> (SELECT selected_truth_plane FROM candidate_params)
    UNION ALL
    SELECT
      'payload_bridge_build_id_mismatch',
      COALESCE((SELECT payload:bridge_build_id::TEXT FROM stage1_payload), '')
        <> (SELECT bridge_build_id FROM candidate_params)
    UNION ALL
    SELECT
      'payload_acris_release_dt_mismatch',
      COALESCE(TRY_TO_DATE((SELECT payload:acris_release_dt::TEXT
                            FROM stage1_payload)), '1900-01-01'::DATE)
        <> (SELECT acris_release_dt FROM candidate_params)
    UNION ALL
    SELECT
      'payload_property_state_mismatch',
      COALESCE((SELECT payload:property_state::TEXT FROM stage1_payload), '')
        <> (SELECT property_state FROM candidate_params)
    UNION ALL
    SELECT
      'payload_collateral_scope_mismatch',
      COALESCE((SELECT payload:collateral_scope::TEXT FROM stage1_payload), '')
        <> (SELECT collateral_scope FROM candidate_params)
    UNION ALL
    SELECT
      'loan_parameter_array_total_section_count_not_one',
      (SELECT total_section_count FROM section_stats) <> 1
    UNION ALL
    SELECT
      'loan_parameter_selected_section_count_not_one',
      (SELECT selected_section_count FROM section_stats) <> 1
    UNION ALL
    SELECT
      'loan_parameter_rows_zero',
      (SELECT loan_parameter_rows FROM loan_parameter_validity) = 0
    UNION ALL
    SELECT
      'loan_parameter_rows_exceed_bound',
      (SELECT loan_parameter_rows FROM loan_parameter_validity)
        > (SELECT max_loan_parameter_rows FROM candidate_params)
    UNION ALL
    SELECT
      'payload_loan_parameter_rows_mismatch',
      (SELECT payload_loan_parameter_rows FROM section_stats)
        <> (SELECT loan_parameter_rows FROM loan_parameter_validity)
    UNION ALL
    SELECT
      'shard_loan_parameter_rows_zero',
      (SELECT shard_loan_parameter_rows FROM shard_counts) = 0
    UNION ALL
    SELECT
      'shard_loan_parameter_rows_exceed_bound',
      (SELECT shard_loan_parameter_rows FROM shard_counts)
        > (SELECT max_shard_loan_parameter_rows FROM candidate_params)
    UNION ALL
    SELECT
      'candidate_borough_rows_exceed_bound',
      (SELECT candidate_borough_rows FROM candidate_counts)
        > (SELECT max_candidate_borough_rows FROM candidate_params)
    UNION ALL
    SELECT
      'loan_parameter_missing_loan_key',
      (SELECT missing_loan_key_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_truth_plane_mismatch',
      (SELECT truth_plane_mismatch_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_property_state_mismatch',
      (SELECT property_state_mismatch_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_collateral_scope_mismatch',
      (SELECT collateral_scope_mismatch_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_invalid_amount',
      (SELECT invalid_amount_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_invalid_originationdate',
      (SELECT invalid_originationdate_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_missing_filed_boroughs',
      (SELECT missing_filed_boroughs_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'loan_parameter_missing_filed_edges',
      (SELECT missing_filed_edges_rows FROM loan_parameter_validity) <> 0
    UNION ALL
    SELECT
      'whole_plane_denominator_does_not_reconcile',
      NOT COALESCE((SELECT whole_plane_reconciles FROM denominator), FALSE)
    UNION ALL
    SELECT
      'shard_candidate_denominator_does_not_reconcile',
      NOT COALESCE((SELECT shard_candidate_reconciles FROM denominator), FALSE)
    UNION ALL
    SELECT
      'candidate_exceeds_shard',
      (SELECT candidate_loans FROM denominator)
        > (SELECT shard_loan_parameter_rows FROM denominator)
  )
  WHERE failed
),
guard_summary AS (
  SELECT IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status
  FROM guard_failures
),
candidate_output AS (
  SELECT
    'h7_stage2_master_party_candidate_row.v0'::TEXT AS row_contract,
    'candidate'::TEXT AS row_kind,
    g.guard_status,
    NULL::TEXT AS guard_failure_reason,
    (SELECT loan_parameter_query_id FROM candidate_params)
      AS upstream_loan_parameter_query_id,
    d.truth_plane AS selected_truth_plane,
    d.shard_count,
    d.shard_index,
    d.shard_partition_expression,
    (SELECT bridge_build_id FROM candidate_params) AS bridge_build_id,
    TO_VARCHAR((SELECT acris_release_dt FROM candidate_params)) AS acris_release_dt,
    (SELECT property_state FROM candidate_params) AS property_state,
    (SELECT collateral_scope FROM candidate_params) AS collateral_scope,
    (SELECT amount_cents_quantization FROM candidate_params)
      AS amount_cents_quantization,
    (SELECT round_amount_lattice_cents FROM candidate_params)
      AS round_amount_lattice_cents,
    (SELECT max_recording_offset_days FROM candidate_params)
      AS max_recording_offset_days,
    d.whole_plane_eligible_loans,
    d.whole_plane_loan_parameter_rows,
    d.shard_loan_parameter_rows,
    d.candidate_loans,
    d.no_candidate_loans,
    d.loan_document_pairs,
    d.candidate_borough_rows,
    d.whole_plane_reconciles,
    d.shard_candidate_reconciles,
    c.loan_key,
    c.assigned_shard_index,
    c.property_keys,
    c.association_plane,
    c.amount_cents,
    TO_VARCHAR(c.originationdate) AS originationdate,
    c.originatorname,
    c.originator_match_text,
    c.filed_counties,
    c.filed_boroughs,
    c.filed_county_borough_edges,
    c.distinct_counts,
    c.diagnostic_county_fips,
    c.bridge_source_record_ids,
    c.document_id,
    c.recorded_borough,
    c.doc_type,
    c.crfn,
    TO_VARCHAR(c.document_date) AS document_date,
    TO_VARCHAR(c.recorded_date) AS recorded_date,
    c.recording_offset_days,
    c.lender_match_text,
    c.lender_party_type,
    c.acris_master_source_record_id,
    c.acris_party_source_record_id
  FROM master_candidates c
  CROSS JOIN denominator d
  CROSS JOIN guard_summary g
  WHERE g.guard_status = 'ok'
),
no_candidate_output AS (
  SELECT
    'h7_stage2_master_party_candidate_row.v0'::TEXT AS row_contract,
    'no_candidate'::TEXT AS row_kind,
    g.guard_status,
    NULL::TEXT AS guard_failure_reason,
    (SELECT loan_parameter_query_id FROM candidate_params)
      AS upstream_loan_parameter_query_id,
    d.truth_plane AS selected_truth_plane,
    d.shard_count,
    d.shard_index,
    d.shard_partition_expression,
    (SELECT bridge_build_id FROM candidate_params) AS bridge_build_id,
    TO_VARCHAR((SELECT acris_release_dt FROM candidate_params)) AS acris_release_dt,
    (SELECT property_state FROM candidate_params) AS property_state,
    (SELECT collateral_scope FROM candidate_params) AS collateral_scope,
    (SELECT amount_cents_quantization FROM candidate_params)
      AS amount_cents_quantization,
    (SELECT round_amount_lattice_cents FROM candidate_params)
      AS round_amount_lattice_cents,
    (SELECT max_recording_offset_days FROM candidate_params)
      AS max_recording_offset_days,
    d.whole_plane_eligible_loans,
    d.whole_plane_loan_parameter_rows,
    d.shard_loan_parameter_rows,
    d.candidate_loans,
    d.no_candidate_loans,
    d.loan_document_pairs,
    d.candidate_borough_rows,
    d.whole_plane_reconciles,
    d.shard_candidate_reconciles,
    n.loan_key,
    n.assigned_shard_index,
    n.property_keys,
    n.association_plane,
    n.amount_cents,
    TO_VARCHAR(n.originationdate) AS originationdate,
    n.originatorname,
    n.originator_match_text,
    n.filed_counties,
    n.filed_boroughs,
    n.filed_county_borough_edges,
    n.distinct_counts,
    n.diagnostic_county_fips,
    n.bridge_source_record_ids,
    NULL::TEXT AS document_id,
    NULL::NUMBER(38,0) AS recorded_borough,
    NULL::TEXT AS doc_type,
    NULL::TEXT AS crfn,
    NULL::TEXT AS document_date,
    NULL::TEXT AS recorded_date,
    NULL::NUMBER(38,0) AS recording_offset_days,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::TEXT AS acris_master_source_record_id,
    NULL::TEXT AS acris_party_source_record_id
  FROM no_candidate_loans n
  CROSS JOIN denominator d
  CROSS JOIN guard_summary g
  WHERE g.guard_status = 'ok'
),
guard_output AS (
  SELECT
    'h7_stage2_master_party_candidate_row.v0'::TEXT AS row_contract,
    'guard_failure'::TEXT AS row_kind,
    g.guard_status,
    f.failure_reason AS guard_failure_reason,
    (SELECT loan_parameter_query_id FROM candidate_params)
      AS upstream_loan_parameter_query_id,
    d.truth_plane AS selected_truth_plane,
    d.shard_count,
    d.shard_index,
    d.shard_partition_expression,
    (SELECT bridge_build_id FROM candidate_params) AS bridge_build_id,
    TO_VARCHAR((SELECT acris_release_dt FROM candidate_params)) AS acris_release_dt,
    (SELECT property_state FROM candidate_params) AS property_state,
    (SELECT collateral_scope FROM candidate_params) AS collateral_scope,
    (SELECT amount_cents_quantization FROM candidate_params)
      AS amount_cents_quantization,
    (SELECT round_amount_lattice_cents FROM candidate_params)
      AS round_amount_lattice_cents,
    (SELECT max_recording_offset_days FROM candidate_params)
      AS max_recording_offset_days,
    d.whole_plane_eligible_loans,
    d.whole_plane_loan_parameter_rows,
    d.shard_loan_parameter_rows,
    d.candidate_loans,
    d.no_candidate_loans,
    d.loan_document_pairs,
    d.candidate_borough_rows,
    d.whole_plane_reconciles,
    d.shard_candidate_reconciles,
    NULL::TEXT AS loan_key,
    NULL::NUMBER(38,0) AS assigned_shard_index,
    NULL::NUMBER(38,0) AS property_keys,
    NULL::TEXT AS association_plane,
    NULL::NUMBER(38,0) AS amount_cents,
    NULL::TEXT AS originationdate,
    NULL::TEXT AS originatorname,
    NULL::TEXT AS originator_match_text,
    NULL::VARIANT AS filed_counties,
    NULL::VARIANT AS filed_boroughs,
    NULL::VARIANT AS filed_county_borough_edges,
    NULL::VARIANT AS distinct_counts,
    NULL::VARIANT AS diagnostic_county_fips,
    NULL::VARIANT AS bridge_source_record_ids,
    NULL::TEXT AS document_id,
    NULL::NUMBER(38,0) AS recorded_borough,
    NULL::TEXT AS doc_type,
    NULL::TEXT AS crfn,
    NULL::TEXT AS document_date,
    NULL::TEXT AS recorded_date,
    NULL::NUMBER(38,0) AS recording_offset_days,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::TEXT AS acris_master_source_record_id,
    NULL::TEXT AS acris_party_source_record_id
  FROM guard_failures f
  CROSS JOIN denominator d
  CROSS JOIN guard_summary g
)
SELECT *
FROM candidate_output
UNION ALL
SELECT *
FROM no_candidate_output
UNION ALL
SELECT *
FROM guard_output
ORDER BY row_kind, loan_key, document_id, recorded_borough;
