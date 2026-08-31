-- Appendix H.7 PIP-block candidate-reach control over accepted truth.
--
-- This reproduces the frozen Gate V2 candidate rule on the fresh H.7
-- population: find every containing parcel for each collateral point without
-- consulting an address, then admit every parcel in the same six-digit BBL
-- block in each pinned MapPLUTO release. Blocks are bounded candidate
-- sections. Their loan-level union is reach accounting only, never a
-- monolithic exact-solver work unit.
--
-- Truth appears only in the final reach comparison. Truth BBLs may not seed
-- `pip_edges`, `pip_blocks`, or `candidate_edges`.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'

WITH
params AS (
  SELECT
    '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'::TEXT
      AS accepted_truth_query_id,
    'h7_staging_accepted_truth_row.v0'::TEXT
      AS expected_accepted_truth_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    200::NUMBER(38,0) AS accepted_truth_row_cap
),
release_pins AS (
  SELECT * FROM VALUES
    ('26v1', '2026-05-01'::DATE, 'shoreline_clipped'),
    ('26v2', '2026-08-01'::DATE, 'shoreline_clipped')
  AS p(release, release_dt, variant)
),
accepted AS (
  SELECT *
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'))
),
accepted_stats AS (
  SELECT
    COUNT(*) AS accepted_rows,
    COUNT(DISTINCT loan_key) AS accepted_loans,
    COUNT_IF(row_contract <>
      (SELECT expected_accepted_truth_contract FROM params))
      AS contract_mismatch_rows,
    COUNT_IF(bridge_build_id <> (SELECT bridge_build_id FROM params))
      AS bridge_build_mismatch_rows,
    COUNT_IF(NOT export_row_cap_reconciles) AS upstream_row_cap_failures,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS truth_bbl_count_mismatch_rows
  FROM accepted
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT 'accepted_truth_result_empty' AS failure_reason,
      (SELECT accepted_rows FROM accepted_stats) = 0 AS failed
    UNION ALL
    SELECT 'accepted_truth_result_exceeds_bound',
      (SELECT accepted_rows FROM accepted_stats)
        > (SELECT accepted_truth_row_cap FROM params)
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
    SELECT 'accepted_truth_upstream_row_cap_failure',
      (SELECT upstream_row_cap_failures FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_bbl_count_mismatch',
      (SELECT truth_bbl_count_mismatch_rows FROM accepted_stats) <> 0
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    MIN(failure_reason) AS refusal_reason
  FROM guard_failures
),
points AS (
  SELECT DISTINCT
    a.loan_key,
    a.truth_plane,
    a.association_plane,
    lip.property_key,
    lip.latitude,
    lip.longitude
  FROM accepted a
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = (SELECT bridge_build_id FROM params)
   AND lip.loan_key = a.loan_key
  WHERE (SELECT guard_status FROM guard_summary) = 'ok'
    AND lip.propertystate = 'NY'
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
),
parcels AS (
  SELECT
    p.release,
    p.release_dt,
    p.variant,
    p.bbl_key,
    SUBSTR(p.bbl_key, 1, 6) AS block_key,
    p.geom_geog,
    p.bbox_xmin,
    p.bbox_ymin,
    p.bbox_xmax,
    p.bbox_ymax
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES p
  JOIN release_pins pin
    ON p.release = pin.release
   AND p.release_dt = pin.release_dt
   AND p.variant = pin.variant
  WHERE p.bbl_key_status = 'valid'
    AND p.key_validation_status = 'valid'
    AND p.bbl_key IS NOT NULL
    AND p.geom_geog IS NOT NULL
),
pip_edges AS (
  SELECT DISTINCT
    p.loan_key,
    p.truth_plane,
    p.association_plane,
    p.property_key,
    parcel.release,
    parcel.release_dt,
    parcel.variant,
    parcel.bbl_key AS pip_bbl,
    parcel.block_key
  FROM points p
  JOIN parcels parcel
    ON p.longitude BETWEEN parcel.bbox_xmin AND parcel.bbox_xmax
   AND p.latitude BETWEEN parcel.bbox_ymin AND parcel.bbox_ymax
   AND ST_CONTAINS(
     parcel.geom_geog,
     ST_MAKEPOINT(p.longitude, p.latitude)
   )
),
pip_blocks AS (
  SELECT DISTINCT
    loan_key,
    truth_plane,
    association_plane,
    property_key,
    release,
    release_dt,
    variant,
    block_key
  FROM pip_edges
),
candidate_edges AS (
  SELECT DISTINCT
    b.loan_key,
    b.truth_plane,
    b.association_plane,
    b.property_key,
    b.block_key,
    b.release,
    b.release_dt,
    b.variant,
    p.bbl_key AS candidate_bbl
  FROM pip_blocks b
  JOIN parcels p
    ON p.release = b.release
   AND p.release_dt = b.release_dt
   AND p.variant = b.variant
   AND p.block_key = b.block_key
),
subject_releases AS (
  SELECT
    a.loan_key,
    a.truth_plane,
    a.association_plane,
    a.truth_bbl_count,
    a.truth_bbls,
    pin.release,
    pin.release_dt,
    pin.variant
  FROM accepted a
  CROSS JOIN release_pins pin
),
truth_edges AS (
  SELECT
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    s.release,
    truth.value::TEXT AS truth_bbl
  FROM subject_releases s,
    LATERAL FLATTEN(input => s.truth_bbls) truth
),
subject_counts AS (
  SELECT
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    s.release,
    s.release_dt,
    s.variant,
    s.truth_bbl_count,
    COUNT(DISTINCT p.property_key) AS property_points,
    COUNT(DISTINCT e.property_key) AS pip_reached_points,
    COUNT(DISTINCT e.block_key) AS pip_blocks,
    COUNT(DISTINCT c.candidate_bbl) AS candidate_bbls
  FROM subject_releases s
  LEFT JOIN points p
    ON p.loan_key = s.loan_key
   AND p.truth_plane = s.truth_plane
   AND p.association_plane = s.association_plane
  LEFT JOIN pip_edges e
    ON e.loan_key = s.loan_key
   AND e.truth_plane = s.truth_plane
   AND e.association_plane = s.association_plane
   AND e.property_key = p.property_key
   AND e.release = s.release
  LEFT JOIN candidate_edges c
    ON c.loan_key = s.loan_key
   AND c.truth_plane = s.truth_plane
   AND c.association_plane = s.association_plane
   AND c.release = s.release
  GROUP BY
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    s.release,
    s.release_dt,
    s.variant,
    s.truth_bbl_count
),
truth_hits AS (
  SELECT
    t.loan_key,
    t.truth_plane,
    t.association_plane,
    t.release,
    COUNT(DISTINCT t.truth_bbl) AS truth_bbls,
    COUNT(DISTINCT IFF(c.candidate_bbl IS NOT NULL, t.truth_bbl, NULL))
      AS reached_truth_bbls
  FROM truth_edges t
  LEFT JOIN candidate_edges c
    ON c.loan_key = t.loan_key
   AND c.truth_plane = t.truth_plane
   AND c.association_plane = t.association_plane
   AND c.release = t.release
   AND c.candidate_bbl = t.truth_bbl
  GROUP BY t.loan_key, t.truth_plane, t.association_plane, t.release
),
subject_reach AS (
  SELECT
    c.*,
    h.reached_truth_bbls,
    CASE
      WHEN h.reached_truth_bbls = c.truth_bbl_count THEN 'full'
      WHEN h.reached_truth_bbls = 0 THEN 'none'
      ELSE 'partial'
    END AS reach_status
  FROM subject_counts c
  JOIN truth_hits h
    ON h.loan_key = c.loan_key
   AND h.truth_plane = c.truth_plane
   AND h.association_plane = c.association_plane
   AND h.release = c.release
)
SELECT
  g.guard_status,
  g.refusal_reason,
  (SELECT accepted_truth_query_id FROM params) AS accepted_truth_query_id,
  'all_release_parcels_sharing_six_digit_block_with_address_blind_pip'
    AS candidate_selector,
  r.truth_plane,
  r.association_plane,
  r.release,
  r.release_dt,
  r.variant,
  COUNT(*) AS accepted_subjects,
  SUM(r.property_points) AS property_points,
  SUM(r.pip_reached_points) AS pip_reached_points,
  SUM(r.pip_blocks) AS pip_block_edges,
  COUNT_IF(r.pip_reached_points = 0) AS no_pip_subjects,
  COUNT_IF(r.reach_status = 'full') AS full_reach_subjects,
  COUNT_IF(r.reach_status = 'partial') AS partial_reach_subjects,
  COUNT_IF(r.reach_status = 'none') AS no_reach_subjects,
  COUNT_IF(r.reached_truth_bbls > r.truth_bbl_count)
    AS reach_accounting_failures,
  SUM(r.truth_bbl_count) AS truth_bbl_edges,
  SUM(r.reached_truth_bbls) AS reached_truth_bbl_edges,
  MIN(r.candidate_bbls) AS min_candidate_bbls,
  MEDIAN(r.candidate_bbls) AS median_candidate_bbls,
  PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY r.candidate_bbls)
    AS p90_candidate_bbls,
  MAX(r.candidate_bbls) AS max_candidate_bbls,
  SUM(r.candidate_bbls) AS candidate_bbl_edges
FROM subject_reach r
CROSS JOIN guard_summary g
GROUP BY
  g.guard_status,
  g.refusal_reason,
  r.truth_plane,
  r.association_plane,
  r.release,
  r.release_dt,
  r.variant
ORDER BY r.truth_plane, r.association_plane, r.release;
