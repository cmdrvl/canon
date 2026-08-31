-- Appendix H.7 Stage 1 bridge-only loan-parameter SQL.
--
-- This is the first of three acquisition-side single-SELECT stages for
-- bd-7bcp. It emits a non-population, non-evidentiary bridge-only payload over
-- the raw-filed NYC collateral slice. It stops before every ACRIS and MapPLUTO
-- table, so it cannot emit master/party candidates, ACRIS LEGALS truth,
-- selected multi-BBL rows, candidate parcels, or solver-ready population rows.
--
-- Coordinator sequence:
--   1. Before submitting this file, byte-substitute the quoted
--      '__BD7BCP_H7_TRUTH_PLANE__' literal with exactly one selected plane:
--   'non_round_amount_date_legal_borough'
--   'round_exact_lender_party'
--      Run this file separately for each plane and preserve separate query
--      receipts.
--   2. Run h7_master_party_candidates.sql with this query id, the same
--      selected plane, and explicit shard_count/shard_index values to produce
--      bounded ACRIS MASTER/PARTY candidates only after its documented
--      physical access-path dependency is repaired. The 0/16 and 0/64 controls
--      both cancelled; do not fan out the current external-table shape.
--   3. Run h7_multi_parcel_legal_residual.sql once per successful Stage-2
--      shard query id with the same selected plane, shard_count, and
--      shard_index to produce bounded LEGALS/MapPLUTO residuals.
--
-- The controls report the full 2,974 two-plane universe. The
-- loan_parameter_array_export contains exactly the selected plane.
--
-- Do not cite this file as the exact SQL text for any 01c6bd* receipt unless a
-- named section is byte/normalization equivalent to the actually executed
-- warehouse query and the receipt carries that section's content hash.
--
-- Load-bearing controls:
-- * Bridge build: 3aed6660-ce1c-46a9-aeb2-7296c134ce8f.
-- * ACRIS release: RELEASE_DT = 2026-08-10.
-- * Filed scope is raw PROPERTYSTATE = 'NY' plus raw PROPERTYCOUNTY mapping.
--   Geocoded COUNTY_FIPS is projected as diagnostic-only metadata.
-- * Mixed-state loans are evaluated only as their NYC filed-collateral slice
--   here; this SQL does not prove full national collateral composition.
-- * Amount equality is exact only after
--   ROUND(value * 100, 0)::NUMBER(38,0).
-- * The round classifier uses the $100k cents lattice
--   MOD(amount_cents, 10000000) = 0, which includes $1m multiples.
-- * The recording offset window is [0,+45] days using ACRIS
--   RECORDED_DATETIME, not address or geocoder metadata.
-- * The exact lender transform is
--   TRIM(REGEXP_REPLACE(UPPER(name), '[^A-Z0-9 ]', ' ')).
--
-- External fresh controls recorded 2026-08-30, retained here as context only:
-- * 01c6bd17-0821-a0dc-006c-c703088c2796, 197 ms, 7 rows:
--   raw PROPERTYSTATE='NY' + filed county reproduced 2,974 loans =
--   653 non-round + 2,321 round. The raw county-only 3,016 and geocoder
--   COUNTY_FIPS 647/2,291 controls are diagnostic-only.
-- * 01c6bd19-0821-9afc-006c-c703088c0936, 313 ms, 2 rows:
--   originator availability drifted from archived G7; preserve discrepancy.
-- * 01c6bd25-0821-a0dc-006c-c703088c27be, 42,031 ms:
--   fresh round candidate aggregation found 2,317 / 311 / 439 versus
--   archived 2,173 / 182 / 277. The cached repeat is not independent.
-- * 01c6bd28-0821-a0dc-006c-c703088c27c6, 45,044 ms:
--   round legal residual cancelled with 000604/57014; no legal counts from
--   that attempt are admissible.
-- * 01c6beec-0821-9afc-006c-c703088ce266, 45s:
--   combined candidate-control shape cancelled; do not retry the combined
--   shape. Use the selected_truth_plane literal and run staged per-plane SQL.
-- * 01c6bef3-0821-9afc-006c-c703088ce26a, 45.05s:
--   non-round plane candidate/control shape cancelled; do not retry that shape.
--   Split MASTER/PARTY candidate acquisition into Stage 2.
-- * 01c6befd-0821-9afc-006c-c703088ce26e, 6.966s:
--   true Stage-1 non-round run succeeded with guard ok and 653 exported rows.
--   01c6befe was only the self-matching query-history control, not Stage 1.
-- * 01c6bf06-0821-9afc-006c-c703088ce28e, 45.084s:
--   alias-fixed unsharded Stage 2 against true Stage 1 timed out after
--   compile 15.525s, execute 29.529s, 954601 bytes scanned; rendered SHA-256
--   7b56597985857254b5e940686a287f71fe2faa5a9fead4bfb6b688c3c17367bb.
--   Later 0/16 and 0/64 controls also cancelled, so logical sharding alone is
--   insufficient; see h7_master_party_candidates.sql for the full receipts.

WITH
params AS (
  SELECT
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    45::NUMBER(9,0) AS max_recording_offset_days,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT AS amount_cents_quantization,
    'TRIM(REGEXP_REPLACE(UPPER(name), ''[^A-Z0-9 ]'', '' ''))'::TEXT
      AS lender_match_transform,
    '__BD7BCP_H7_TRUTH_PLANE__'::TEXT AS selected_truth_plane,
    3000::NUMBER(38,0) AS max_selected_loan_parameter_rows
),
filed_county_map AS (
  SELECT * FROM VALUES
    ('NEW YORK', 1), ('MANHATTAN', 1), ('NY061', 1),
    ('BRONX', 2),
    ('KINGS', 3), ('BROOKLYN', 3),
    ('QUEENS', 4),
    ('RICHMOND', 5)
  AS m(propertycounty, filed_borough)
),
bridge_rows AS (
  SELECT
    lip.loan_key,
    lip.property_key,
    lip.originatorname,
    lip.originator_match_text,
    lip.originationdate,
    ROUND(lip.originalloanamount * 100, 0)::NUMBER(38,0) AS amount_cents,
    lip.propertystate AS property_state,
    lip.propertycounty AS propertycounty_raw,
    UPPER(TRIM(lip.propertycounty)) AS propertycounty_norm,
    lip.county_fips AS geocoded_county_fips,
    m.filed_borough,
    lip.loan_property_count
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
  JOIN params p ON lip.build_id = p.bridge_build_id
  LEFT JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
),
ny_filed_bridge_rows AS (
  SELECT *
  FROM bridge_rows
  WHERE property_state = (SELECT property_state FROM params)
    AND filed_borough IS NOT NULL
),
loan_counts AS (
  SELECT
    loan_key,
    COUNT(DISTINCT property_key) AS property_keys,
    COUNT(DISTINCT IFF(property_state IS NOT NULL, property_state, NULL)) AS distinct_property_state,
    COUNT(DISTINCT IFF(property_state = 'NY', property_state, NULL)) AS distinct_ny_property_state,
    COUNT(DISTINCT filed_borough) AS distinct_filed_borough,
    COUNT(DISTINCT IFF(originatorname IS NOT NULL, originatorname, NULL)) AS distinct_originatorname,
    COUNT(DISTINCT IFF(originator_match_text IS NOT NULL, originator_match_text, NULL)) AS distinct_originator_match_text,
    COUNT(DISTINCT IFF(originationdate IS NOT NULL, originationdate, NULL)) AS distinct_originationdate,
    COUNT(DISTINCT IFF(amount_cents IS NOT NULL, amount_cents, NULL)) AS distinct_originalloanamount,
    COUNT(DISTINCT IFF(propertycounty_norm IS NOT NULL, propertycounty_norm, NULL)) AS distinct_filed_county,
    MAX(originationdate) AS max_originationdate,
    MAX(amount_cents) AS max_amount_cents,
    MAX(originatorname) AS max_originatorname,
    MAX(originator_match_text) AS max_originator_match_text,
    ARRAY_AGG(DISTINCT propertycounty_norm) WITHIN GROUP (ORDER BY propertycounty_norm) AS filed_counties,
    ARRAY_AGG(DISTINCT filed_borough) WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs,
    ARRAY_AGG(DISTINCT geocoded_county_fips) WITHIN GROUP (ORDER BY geocoded_county_fips) AS diagnostic_county_fips
  FROM ny_filed_bridge_rows
  GROUP BY loan_key
),
loan_filed_county_edges AS (
  SELECT
    loan_key,
    ARRAY_AGG(
      OBJECT_CONSTRUCT(
        'filed_county', propertycounty_norm,
        'filed_borough', filed_borough
      )
    ) WITHIN GROUP (ORDER BY propertycounty_norm, filed_borough) AS filed_county_borough_edges
  FROM (
    SELECT DISTINCT loan_key, propertycounty_norm, filed_borough
    FROM ny_filed_bridge_rows
  )
  GROUP BY loan_key
),
loan_bridge_source_records AS (
  SELECT
    loan_key,
    ARRAY_AGG(source_record_id) WITHIN GROUP (ORDER BY source_record_id)
      AS bridge_source_record_ids
  FROM (
    SELECT DISTINCT
      loan_key,
      'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:'
        || (SELECT bridge_build_id FROM params)
        || ':'
        || loan_key
        || ':'
        || COALESCE(TO_VARCHAR(property_key), '<null>') AS source_record_id
    FROM ny_filed_bridge_rows
  )
  GROUP BY loan_key
),
loan_gate AS (
  SELECT
    c.loan_key,
    c.property_keys,
    c.distinct_property_state,
    c.distinct_ny_property_state,
    c.distinct_filed_county,
    c.distinct_filed_borough,
    c.distinct_originatorname,
    c.distinct_originator_match_text,
    c.distinct_originationdate,
    c.distinct_originalloanamount,
    IFF(c.distinct_originationdate = 1, c.max_originationdate, NULL) AS originationdate,
    IFF(c.distinct_originalloanamount = 1, c.max_amount_cents, NULL) AS amount_cents,
    IFF(c.distinct_originatorname = 1, c.max_originatorname, NULL) AS originatorname,
    IFF(c.distinct_originator_match_text = 1, c.max_originator_match_text, NULL) AS originator_match_text,
    c.filed_counties,
    c.filed_boroughs,
    e.filed_county_borough_edges,
    r.bridge_source_record_ids,
    c.diagnostic_county_fips
  FROM loan_counts c
  JOIN loan_filed_county_edges e
    ON e.loan_key = c.loan_key
  JOIN loan_bridge_source_records r
    ON r.loan_key = c.loan_key
),
loan_classification AS (
  SELECT
    *,
    CASE
      WHEN distinct_originationdate = 0 THEN 'discard_missing_originationdate'
      WHEN distinct_originationdate > 1 THEN 'discard_ambiguous_originationdate'
      WHEN distinct_originalloanamount = 0 THEN 'discard_missing_originalloanamount'
      WHEN distinct_originalloanamount > 1 THEN 'discard_ambiguous_originalloanamount'
      WHEN amount_cents = 0 THEN 'discard_zero_originalloanamount'
      WHEN distinct_originatorname > 1 THEN 'discard_ambiguous_originatorname'
      WHEN distinct_originator_match_text > 1 THEN 'discard_ambiguous_originator_match_text'
      WHEN distinct_filed_borough = 0 THEN 'discard_no_mapped_filed_borough'
      ELSE 'admissible_for_plane_classification'
    END AS gate_status,
    CASE
      WHEN amount_cents IS NULL THEN NULL
      WHEN MOD(amount_cents, (SELECT round_amount_lattice_cents FROM params)) = 0
        THEN 'round_exact_lender_party'
      ELSE 'non_round_amount_date_legal_borough'
    END AS truth_plane
  FROM loan_gate
),
selected_loan_classification AS (
  SELECT *
  FROM loan_classification
  WHERE gate_status = 'admissible_for_plane_classification'
    AND truth_plane = (SELECT selected_truth_plane FROM params)
),
ny_universe_control AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans
  FROM loan_classification
  WHERE gate_status = 'admissible_for_plane_classification'
  GROUP BY truth_plane
),
county_only_diagnostic AS (
  SELECT
    COALESCE(property_state, '<null>') AS property_state,
    COUNT(DISTINCT loan_key) AS loans
  FROM bridge_rows
  WHERE filed_borough IS NOT NULL
  GROUP BY property_state
),
geocoder_county_fips_diagnostic AS (
  SELECT
    l.truth_plane,
    COUNT(DISTINCT b.loan_key) AS loans
  FROM bridge_rows b
  JOIN loan_classification l
    ON l.loan_key = b.loan_key
  WHERE b.geocoded_county_fips IN ('36005','36047','36061','36081','36085')
    AND b.property_state = (SELECT property_state FROM params)
    AND l.gate_status = 'admissible_for_plane_classification'
  GROUP BY l.truth_plane
),
originator_availability_diagnostic AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans,
    COUNT_IF(distinct_originatorname = 1) AS raw_originator_available,
    COUNT_IF(distinct_originator_match_text = 1) AS originator_match_text_available,
    COUNT_IF(distinct_originatorname = 0) AS raw_originator_absent,
    COUNT_IF(distinct_originator_match_text = 0) AS originator_match_text_absent
  FROM loan_classification
  WHERE gate_status = 'admissible_for_plane_classification'
  GROUP BY truth_plane
),
loan_parameter_array_export AS (
  SELECT
    truth_plane,
    COUNT(*) AS loan_parameter_rows,
    ARRAY_AGG(
      OBJECT_CONSTRUCT_KEEP_NULL(
        'loan_key', loan_key,
        'truth_plane', truth_plane,
        'property_state', (SELECT property_state FROM params),
        'collateral_scope', (SELECT collateral_scope FROM params),
        'filed_counties', filed_counties,
        'filed_boroughs', filed_boroughs,
        'filed_county_borough_edges', filed_county_borough_edges,
        'property_keys', property_keys,
        'association_plane', IFF(property_keys > 1, 'multi_property', 'single_property'),
        'amount_cents', amount_cents,
        'originationdate', TO_VARCHAR(originationdate),
        'originatorname', originatorname,
        'originator_match_text', originator_match_text,
        'distinct_counts', OBJECT_CONSTRUCT(
          'originatorname', distinct_originatorname,
          'originator_match_text', distinct_originator_match_text,
          'originationdate', distinct_originationdate,
          'originalloanamount', distinct_originalloanamount,
          'filed_county', distinct_filed_county,
          'filed_borough', distinct_filed_borough
        ),
        'diagnostic_county_fips', diagnostic_county_fips,
        'bridge_source_record_ids', bridge_source_record_ids
      )
    ) WITHIN GROUP (ORDER BY loan_key) AS loan_parameter_rows_payload
  FROM selected_loan_classification
  WHERE truth_plane = (SELECT selected_truth_plane FROM params)
  GROUP BY truth_plane
),
loan_parameter_guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT
      'invalid_selected_truth_plane' AS failure_reason,
      (SELECT selected_truth_plane FROM params) NOT IN (
        'non_round_amount_date_legal_borough',
        'round_exact_lender_party'
      ) AS failed
    UNION ALL
    SELECT
      'loan_parameter_array_export_section_count_not_one',
      (SELECT COUNT(*) FROM loan_parameter_array_export) <> 1
    UNION ALL
    SELECT
      'loan_parameter_array_export_truth_plane_mismatch',
      COALESCE((SELECT MAX(truth_plane) FROM loan_parameter_array_export), '')
        <> (SELECT selected_truth_plane FROM params)
    UNION ALL
    SELECT
      'loan_parameter_rows_zero',
      COALESCE((SELECT MAX(loan_parameter_rows) FROM loan_parameter_array_export), 0) = 0
    UNION ALL
    SELECT
      'loan_parameter_rows_exceed_bound',
      COALESCE((SELECT MAX(loan_parameter_rows) FROM loan_parameter_array_export), 0)
        > (SELECT max_selected_loan_parameter_rows FROM params)
  )
  WHERE failed
)
SELECT
  OBJECT_CONSTRUCT(
    'payload_kind', 'h7_bridge_loan_parameters.v0',
    'payload_contract', 'h7_bridge_loan_parameters.v0',
    'payload_purpose', 'stage1_bridge_only_eligible_loan_parameters',
    'is_population_rows_contract', FALSE,
    'is_evidentiary_legal_residual', FALSE,
    'contains_acris_tables', FALSE,
    'contains_mappluto_tables', FALSE,
    'bridge_build_id', (SELECT bridge_build_id FROM params),
    'acris_release_dt', TO_VARCHAR((SELECT acris_release_dt FROM params)),
    'property_state', (SELECT property_state FROM params),
    'collateral_scope', (SELECT collateral_scope FROM params),
    'selected_truth_plane', (SELECT selected_truth_plane FROM params),
    'loan_parameter_guard_status',
      IFF((SELECT COUNT(*) FROM loan_parameter_guard_failures) = 0, 'ok', 'refused'),
    'loan_parameter_guard_failures', COALESCE(
      (
        SELECT ARRAY_AGG(failure_reason) WITHIN GROUP (ORDER BY failure_reason)
        FROM loan_parameter_guard_failures
      ),
      ARRAY_CONSTRUCT()
    ),
    'amount_cents_quantization', (SELECT amount_cents_quantization FROM params),
    'round_amount_lattice_cents', (SELECT round_amount_lattice_cents FROM params),
    'recording_offset_window_days', OBJECT_CONSTRUCT(
      'min_days', 0,
      'max_days', (SELECT max_recording_offset_days FROM params)
    ),
    'lender_match_transform', (SELECT lender_match_transform FROM params),
    'source_tables', ARRAY_CONSTRUCT(
      'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY'
    ),
    'controls', OBJECT_CONSTRUCT(
      'raw_property_state_ny_universe', (
        SELECT ARRAY_AGG(
          OBJECT_CONSTRUCT(
            'truth_plane', truth_plane,
            'eligible_loans', eligible_loans
          )
        ) WITHIN GROUP (ORDER BY truth_plane)
        FROM ny_universe_control
      ),
      'county_only_diagnostic', (
        SELECT ARRAY_AGG(
          OBJECT_CONSTRUCT(
            'property_state', property_state,
            'loans', loans
          )
        ) WITHIN GROUP (ORDER BY property_state)
        FROM county_only_diagnostic
      ),
      'geocoder_county_fips_diagnostic', (
        SELECT ARRAY_AGG(
          OBJECT_CONSTRUCT(
            'truth_plane', truth_plane,
            'loans', loans
          )
        ) WITHIN GROUP (ORDER BY truth_plane)
        FROM geocoder_county_fips_diagnostic
      ),
      'originator_availability_diagnostic', (
        SELECT ARRAY_AGG(
          OBJECT_CONSTRUCT(
            'truth_plane', truth_plane,
            'eligible_loans', eligible_loans,
            'raw_originator_available', raw_originator_available,
            'originator_match_text_available', originator_match_text_available,
            'raw_originator_absent', raw_originator_absent,
            'originator_match_text_absent', originator_match_text_absent
          )
        ) WITHIN GROUP (ORDER BY truth_plane)
        FROM originator_availability_diagnostic
      )
    ),
    'loan_parameter_array_export', (
      SELECT COALESCE(
        ARRAY_AGG(
          OBJECT_CONSTRUCT(
            'truth_plane', truth_plane,
            'loan_parameter_rows', loan_parameter_rows,
            'loan_parameter_rows_payload', loan_parameter_rows_payload
          )
        ) WITHIN GROUP (ORDER BY truth_plane),
        ARRAY_CONSTRUCT()
      )
      FROM loan_parameter_array_export
    ),
    'loan_parameter_export_contract', OBJECT_CONSTRUCT(
      'selected_truth_plane', (SELECT selected_truth_plane FROM params),
      'cardinality', 'loan_parameter_array_export must contain exactly one section and it must match selected_truth_plane; raw_property_state_ny_universe remains the full two-plane 2,974 control',
      'stage_boundary', 'No ACRIS or MapPLUTO tables are read in Stage 1; Stage 2 performs shard-bounded MASTER/PARTY candidate acquisition from this selected loan-parameter array.'
    ),
    'selected_loan_parameter_denominator', COALESCE(
      (
        SELECT OBJECT_CONSTRUCT(
            'truth_plane', truth_plane,
            'loan_parameter_rows', loan_parameter_rows
          )
        FROM loan_parameter_array_export
      ),
      OBJECT_CONSTRUCT(
        'truth_plane', (SELECT selected_truth_plane FROM params),
        'loan_parameter_rows', 0
      )
    ),
    'limitation', 'This payload stops before every ACRIS and MapPLUTO table; it is not canon_geo_h7_population_rows.v0 and cannot prove candidate reach, accepted legal truth, or selected multi-parcel truth.',
    'next_step', 'Run h7_master_party_candidates.sql for this same selected_truth_plane and each explicit shard_count/shard_index: byte-substitute its loan-parameter query-id, truth-plane, shard-count, and shard-index sentinel string literals before submission.'
  ) AS h7_loan_parameter_payload;
