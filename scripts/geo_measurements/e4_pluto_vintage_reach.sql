-- bd-1q5y: PLUTO lot-vintage reach probe for the frozen retained E4 population.
--
-- Input relation:
--   missing_truth_bbls(case_id, truth_plane, bbl)
--
-- The retained 2026-09-08 measurement generated that relation from
-- scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/
-- population_request_roll_universe.json.gz by taking truth parcels absent from
-- the single-release candidate universe. Keep that denominator frozen; do not
-- narrow it after observing PLUTO hits.
--
-- Source contracts checked before use:
-- * NYC_DCP_PLUTO_LOT_VINTAGES is clustered LINEAR(release, borough, bbl).
--   The query binds releases through NYC_DCP_PLUTO_MANIFEST_EXT, then joins by
--   release and canonical BBL. It intentionally does not filter to current
--   rows, because the measured hypothesis is "present under any release".
-- * NYC_DCP_PLUTO_MANIFEST_EXT carries release raw ZIP SHA256 values.
-- * NYC_DCP_PAD_BBL_HOT is clustered LINEAR(release, bbl_key). The residual
--   attribution probe binds release='26B' before testing PAD condo/billing
--   rows.
WITH
probe_releases AS (
  SELECT
    release,
    release_dt,
    payload:artifacts[0]:sha256::TEXT AS source_zip_sha256,
    payload:artifacts[0]:s3_key::TEXT AS source_zip_s3_key,
    payload:artifacts[0]:license_terms::TEXT AS license_terms
  FROM EDGAR_DB.SOURCE.NYC_DCP_PLUTO_MANIFEST_EXT
  WHERE dataset = 'pluto'
),
normalized_pluto AS (
  SELECT
    REGEXP_REPLACE(v.bbl, '\\.00$', '') AS canonical_bbl,
    v.release,
    v.release_dt,
    v.source_row_number,
    v.source_filename,
    v.is_current_release,
    r.source_zip_sha256,
    r.source_zip_s3_key,
    r.license_terms
  FROM EDGAR_DB.SOURCE.NYC_DCP_PLUTO_LOT_VINTAGES v
  JOIN probe_releases r
    ON r.release = v.release
   AND r.release_dt = v.release_dt
  WHERE CASE v.borough
      WHEN 'MN' THEN '1'
      WHEN 'BX' THEN '2'
      WHEN 'BK' THEN '3'
      WHEN 'QN' THEN '4'
      WHEN 'SI' THEN '5'
      ELSE v.borough
    END IN ('1', '2', '3', '4', '5')
),
pluto_hits AS (
  SELECT
    m.case_id,
    m.truth_plane,
    m.bbl,
    COUNT(*) AS vintage_row_count,
    COUNT_IF(p.is_current_release) AS current_release_rows,
    MIN(p.release_dt) AS first_release_dt,
    MAX(p.release_dt) AS last_release_dt,
    LISTAGG(DISTINCT p.release, '|') WITHIN GROUP (ORDER BY p.release) AS releases,
    MIN(p.source_row_number) AS min_source_row_number,
    MAX(p.source_row_number) AS max_source_row_number,
    MIN(p.source_filename) AS example_source_filename,
    LISTAGG(DISTINCT p.source_zip_sha256, '|')
      WITHIN GROUP (ORDER BY p.source_zip_sha256) AS source_zip_sha256_values
  FROM missing_truth_bbls m
  JOIN normalized_pluto p
    ON p.canonical_bbl = m.bbl
  GROUP BY m.case_id, m.truth_plane, m.bbl
),
case_reach AS (
  SELECT
    m.case_id,
    m.truth_plane,
    COUNT(DISTINCT m.bbl) AS missing_truth_bbls_before,
    COUNT(DISTINCT h.bbl) AS missing_truth_bbls_recovered_by_vintage,
    COUNT(DISTINCT m.bbl) - COUNT(DISTINCT h.bbl) AS missing_truth_bbls_after
  FROM missing_truth_bbls m
  LEFT JOIN pluto_hits h
    ON h.case_id = m.case_id
   AND h.bbl = m.bbl
  GROUP BY m.case_id, m.truth_plane
),
residual_bbls AS (
  SELECT DISTINCT
    m.bbl
  FROM missing_truth_bbls m
  LEFT JOIN pluto_hits h
    ON h.case_id = m.case_id
   AND h.bbl = m.bbl
  WHERE h.bbl IS NULL
),
pad_residual_probe AS (
  SELECT
    r.bbl,
    p.release,
    p.release_dt,
    p.source_row_number,
    p.bbl_key,
    p.low_bbl_key,
    p.high_bbl_key,
    p.billing_bbl_key,
    p.condo_flag,
    p.condo_number,
    p.source_zip_sha256,
    p.parser_version,
    p.license_terms,
    p.attribution_text,
    p.source_filename,
    CASE
      WHEN p.bbl_key = r.bbl THEN 'exact_bbl_key'
      WHEN p.low_bbl_key <= r.bbl AND p.high_bbl_key >= r.bbl THEN 'range_contains'
      ELSE 'unmatched'
    END AS pad_match_kind
  FROM residual_bbls r
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT p
    ON p.release = '26B'
   AND SUBSTR(r.bbl, 1, 1) = TO_VARCHAR(p.boro)
   AND (
      p.bbl_key = r.bbl
      OR (p.low_bbl_key <= r.bbl AND p.high_bbl_key >= r.bbl)
    )
)
SELECT
  (SELECT COUNT(*) FROM missing_truth_bbls) AS missing_truth_bbl_refs,
  (SELECT COUNT(DISTINCT bbl) FROM missing_truth_bbls) AS unique_missing_truth_bbls,
  (SELECT COUNT(DISTINCT bbl) FROM pluto_hits) AS present_any_vintage_unique_missing_bbls,
  (SELECT COUNT(DISTINCT bbl) FROM residual_bbls) AS absent_all_pluto_vintages_unique_missing_bbls,
  (SELECT COUNT(DISTINCT bbl) FROM pluto_hits WHERE current_release_rows > 0)
    AS present_current_unique_missing_bbls,
  (SELECT COUNT(DISTINCT bbl) FROM pluto_hits WHERE current_release_rows = 0)
    AS historical_only_unique_missing_bbls,
  (SELECT COUNT(DISTINCT bbl) FROM pad_residual_probe WHERE pad_match_kind != 'unmatched')
    AS residual_bbls_in_pad_26b,
  (SELECT COUNT(DISTINCT bbl) FROM pad_residual_probe WHERE pad_match_kind = 'range_contains')
    AS residual_bbls_in_condo_range,
  (SELECT COUNT(DISTINCT bbl) FROM pad_residual_probe WHERE billing_bbl_key IS NOT NULL)
    AS residual_bbls_with_billing_bbl;
