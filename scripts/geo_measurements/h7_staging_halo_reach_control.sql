-- Appendix H.7 r8+k1 candidate-reach control over accepted truth.
--
-- This query consumes one successful h7_staging_accepted_truth_row.v0 result,
-- derives one r8 home cell per in-scope collateral point, expands each point to
-- a k1 work section, and measures whether the union of those bounded sections
-- contains the accepted ACRIS truth BBLs in each pinned MapPLUTO release.
--
-- The loan-level union is a reach diagnostic, not a solver work unit. Exact
-- solving must remain section -> incidence components -> exact residuals; it
-- must never receive the thousands of unioned parcel keys as one monolithic
-- problem. H3 supplies blocking only, never truth. Snowflake's H3 assignments
-- are empirical warehouse measurements and require h3o replay before becoming
-- Canon home-cell artifacts.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'
--   __BD2B9D_H7_HALO_K__

WITH
params AS (
  SELECT
    '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'::TEXT
      AS accepted_truth_query_id,
    'h7_staging_accepted_truth_row.v0'::TEXT
      AS expected_accepted_truth_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    8::NUMBER(9,0) AS h3_resolution,
    __BD2B9D_H7_HALO_K__::NUMBER(9,0) AS halo_k,
    200::NUMBER(38,0) AS accepted_truth_row_cap
),
release_pins AS (
  SELECT * FROM VALUES
    ('26v1', '2026-05-01'::DATE, 'shoreline_clipped'),
    ('26v2', '2026-08-01'::DATE, 'shoreline_clipped')
  AS p(release, release_dt, variant)
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
    a.truth_bbl_count,
    a.truth_bbls,
    lip.property_key,
    lip.latitude,
    lip.longitude,
    m.filed_borough,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(lip.longitude, lip.latitude),
      (SELECT h3_resolution FROM params)
    ) AS home_cell
  FROM accepted a
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = (SELECT bridge_build_id FROM params)
   AND lip.loan_key = a.loan_key
  JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
  WHERE (SELECT guard_status FROM guard_summary) = 'ok'
    AND lip.propertystate = 'NY'
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
    AND ARRAY_CONTAINS(m.filed_borough::VARIANT, a.filed_boroughs)
),
point_work_cells AS (
  SELECT DISTINCT
    p.loan_key,
    p.truth_plane,
    p.property_key,
    p.home_cell,
    H3_STRING_TO_INT(cell.value::TEXT) AS work_cell_int
  FROM points p,
    LATERAL FLATTEN(
      input => H3_GRID_DISK(
        p.home_cell,
        (SELECT halo_k FROM params)
      )
    ) cell
),
parcel_index AS (
  SELECT
    h.release,
    h.release_dt,
    h.feature_type AS variant,
    h.bbl_key,
    h.h3_r8_int
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_GEOMETRY_HOT_KEYS h
  JOIN release_pins pin
    ON h.release = pin.release
   AND h.release_dt = pin.release_dt
   AND h.feature_type = pin.variant
  WHERE h.source_system = 'nyc_dcp'
    AND h.source_table = 'nyc_dcp_mappluto_hot'
    AND h.dataset = 'mappluto'
    AND h.state_key = 'NY'
    AND h.bbl_key_status = 'valid'
    AND h.h3_r8_status = 'valid'
    AND h.key_validation_status = 'valid'
    AND h.bbl_key IS NOT NULL
    AND h.h3_r8_int IS NOT NULL
),
section_candidate_edges AS (
  SELECT DISTINCT
    w.loan_key,
    w.truth_plane,
    w.property_key,
    w.home_cell,
    p.release,
    p.release_dt,
    p.variant,
    p.bbl_key AS candidate_bbl
  FROM point_work_cells w
  JOIN parcel_index p
    ON p.h3_r8_int = w.work_cell_int
),
candidate_edges AS (
  SELECT DISTINCT
    loan_key,
    truth_plane,
    release,
    release_dt,
    variant,
    candidate_bbl
  FROM section_candidate_edges
),
subject_releases AS (
  SELECT
    a.loan_key,
    a.truth_plane,
    a.truth_bbl_count,
    a.truth_bbls,
    p.release,
    p.release_dt,
    p.variant
  FROM accepted a
  CROSS JOIN release_pins p
),
section_releases AS (
  SELECT
    p.loan_key,
    p.truth_plane,
    p.property_key,
    p.home_cell,
    pin.release,
    pin.release_dt,
    pin.variant
  FROM points p
  CROSS JOIN release_pins pin
),
section_candidate_counts AS (
  SELECT
    loan_key,
    truth_plane,
    property_key,
    home_cell,
    release,
    COUNT(*) AS section_candidate_bbls
  FROM section_candidate_edges
  GROUP BY
    loan_key,
    truth_plane,
    property_key,
    home_cell,
    release
),
section_summary AS (
  SELECT
    s.truth_plane,
    s.release,
    COUNT(*) AS work_sections,
    COUNT_IF(COALESCE(c.section_candidate_bbls, 0) = 0)
      AS empty_work_sections,
    MIN(COALESCE(c.section_candidate_bbls, 0))
      AS min_section_candidate_bbls,
    MEDIAN(COALESCE(c.section_candidate_bbls, 0))
      AS median_section_candidate_bbls,
    PERCENTILE_CONT(0.9) WITHIN GROUP (
      ORDER BY COALESCE(c.section_candidate_bbls, 0)
    ) AS p90_section_candidate_bbls,
    MAX(COALESCE(c.section_candidate_bbls, 0))
      AS max_section_candidate_bbls
  FROM section_releases s
  LEFT JOIN section_candidate_counts c
    ON c.loan_key = s.loan_key
   AND c.truth_plane = s.truth_plane
   AND c.property_key = s.property_key
   AND c.home_cell = s.home_cell
   AND c.release = s.release
  GROUP BY s.truth_plane, s.release
),
candidate_counts AS (
  SELECT
    loan_key,
    truth_plane,
    release,
    COUNT(*) AS candidate_bbl_count
  FROM candidate_edges
  GROUP BY loan_key, truth_plane, release
),
truth_edges AS (
  SELECT
    s.loan_key,
    s.truth_plane,
    s.release,
    truth.value::TEXT AS truth_bbl
  FROM subject_releases s,
    LATERAL FLATTEN(input => s.truth_bbls) truth
),
truth_hits AS (
  SELECT
    t.loan_key,
    t.truth_plane,
    t.release,
    COUNT(*) AS truth_bbl_count,
    COUNT_IF(c.candidate_bbl IS NOT NULL) AS reached_truth_bbls
  FROM truth_edges t
  LEFT JOIN candidate_edges c
    ON c.loan_key = t.loan_key
   AND c.truth_plane = t.truth_plane
   AND c.release = t.release
   AND c.candidate_bbl = t.truth_bbl
  GROUP BY t.loan_key, t.truth_plane, t.release
),
subject_reach AS (
  SELECT
    s.loan_key,
    s.truth_plane,
    s.release,
    s.release_dt,
    s.variant,
    s.truth_bbl_count,
    COALESCE(c.candidate_bbl_count, 0) AS candidate_bbl_count,
    h.reached_truth_bbls,
    CASE
      WHEN h.reached_truth_bbls = s.truth_bbl_count THEN 'full'
      WHEN h.reached_truth_bbls = 0 THEN 'none'
      ELSE 'partial'
    END AS reach_status
  FROM subject_releases s
  LEFT JOIN candidate_counts c
    ON c.loan_key = s.loan_key
   AND c.truth_plane = s.truth_plane
   AND c.release = s.release
  JOIN truth_hits h
    ON h.loan_key = s.loan_key
   AND h.truth_plane = s.truth_plane
   AND h.release = s.release
),
point_counts AS (
  SELECT
    truth_plane,
    COUNT(*) AS property_points,
    COUNT(DISTINCT home_cell) AS distinct_home_cells,
    MIN(latitude) AS min_latitude,
    MAX(latitude) AS max_latitude,
    MIN(longitude) AS min_longitude,
    MAX(longitude) AS max_longitude
  FROM points
  GROUP BY truth_plane
)
SELECT
  g.guard_status,
  g.refusal_reason,
  (SELECT accepted_truth_query_id FROM params) AS accepted_truth_query_id,
  (SELECT h3_resolution FROM params) AS h3_resolution,
  (SELECT halo_k FROM params) AS halo_k,
  'centroid_home_cell_in_point_k' || (SELECT halo_k FROM params)::TEXT
    AS candidate_selector,
  r.truth_plane,
  r.release,
  r.release_dt,
  r.variant,
  COUNT(*) AS accepted_subjects,
  p.property_points,
  p.distinct_home_cells,
  p.min_latitude,
  p.max_latitude,
  p.min_longitude,
  p.max_longitude,
  ss.work_sections,
  ss.empty_work_sections,
  ss.min_section_candidate_bbls,
  ss.median_section_candidate_bbls,
  ss.p90_section_candidate_bbls,
  ss.max_section_candidate_bbls,
  COUNT_IF(r.reach_status = 'full') AS full_reach_subjects,
  COUNT_IF(r.reach_status = 'partial') AS partial_reach_subjects,
  COUNT_IF(r.reach_status = 'none') AS no_reach_subjects,
  SUM(r.truth_bbl_count) AS truth_bbl_edges,
  SUM(r.reached_truth_bbls) AS reached_truth_bbl_edges,
  MIN(r.candidate_bbl_count) AS min_union_candidate_bbls,
  MEDIAN(r.candidate_bbl_count) AS median_union_candidate_bbls,
  PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY r.candidate_bbl_count)
    AS p90_union_candidate_bbls,
  MAX(r.candidate_bbl_count) AS max_union_candidate_bbls,
  SUM(r.candidate_bbl_count) AS union_candidate_bbl_edges
FROM subject_reach r
JOIN point_counts p USING (truth_plane)
JOIN section_summary ss
  ON ss.truth_plane = r.truth_plane
 AND ss.release = r.release
CROSS JOIN guard_summary g
GROUP BY
  g.guard_status,
  g.refusal_reason,
  r.truth_plane,
  r.release,
  r.release_dt,
  r.variant,
  p.property_points,
  p.distinct_home_cells,
  p.min_latitude,
  p.max_latitude,
  p.min_longitude,
  p.max_longitude,
  ss.work_sections,
  ss.empty_work_sections,
  ss.min_section_candidate_bbls,
  ss.median_section_candidate_bbls,
  ss.p90_section_candidate_bbls,
  ss.max_section_candidate_bbls
ORDER BY r.truth_plane, r.release;
