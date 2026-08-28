WITH parcel_cells AS (
  SELECT H3_R8 AS h3_r8_int,
         COUNT(DISTINCT NULLIF(BBL, '')) AS parcel_count
  FROM SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE RELEASE = '26v1'
    AND RELEASE_DT = '2026-05-01'
    AND H3_R8 IS NOT NULL
    AND BBL IS NOT NULL
  GROUP BY H3_R8
), footprint_cells AS (
  SELECT H3_STRING_TO_INT(H3_R8) AS h3_r8_int,
         COUNT(DISTINCT OBJECTID) AS footprint_count
  FROM SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE RELEASE_DT = '2026-08-09'
    AND IS_ACTIVE_FOOTPRINT = TRUE
    AND H3_R8 IS NOT NULL
    AND OBJECTID IS NOT NULL
  GROUP BY H3_R8
), footprint_totals AS (
  SELECT COUNT(DISTINCT OBJECTID) AS active_footprint_denominator,
         COUNT(DISTINCT IFF(H3_R8 IS NULL, OBJECTID, NULL)) AS null_h3_footprints
  FROM SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE RELEASE_DT = '2026-08-09'
    AND IS_ACTIVE_FOOTPRINT = TRUE
    AND OBJECTID IS NOT NULL
), cells AS (
  SELECT p.h3_r8_int,
         p.parcel_count,
         COALESCE(f.footprint_count, 0) AS footprint_count,
         p.parcel_count + COALESCE(f.footprint_count, 0) AS total_features
  FROM parcel_cells p
  LEFT JOIN footprint_cells f USING (h3_r8_int)
)
SELECT
  COUNT(*) AS parcel_containing_cell_count,
  SUM(parcel_count) AS parcel_denominator,
  MIN(parcel_count) AS parcel_min,
  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY parcel_count) AS parcel_median,
  ROUND(AVG(parcel_count), 2) AS parcel_mean,
  PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY parcel_count) AS parcel_p90,
  PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY parcel_count) AS parcel_p99,
  MAX(parcel_count) AS parcel_max,
  SUM(footprint_count) AS footprint_denominator_in_parcel_cells,
  (SELECT active_footprint_denominator FROM footprint_totals)
    AS active_footprint_denominator,
  (SELECT active_footprint_denominator FROM footprint_totals)
    - SUM(footprint_count) AS footprints_outside_parcel_home_cells,
  (SELECT null_h3_footprints FROM footprint_totals) AS null_h3_footprints,
  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY footprint_count) AS footprint_median,
  MAX(footprint_count) AS footprint_max,
  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY total_features) AS total_feature_median,
  PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY total_features) AS total_feature_p99,
  MAX(total_features) AS total_feature_max,
  COUNT_IF(parcel_count < 1 OR footprint_count < 0) AS invalid_count_cells
FROM cells;

-- Expected 2026-08-28:
-- cells=1192 parcels=856614 parcel min/median/mean/p90/p99/max=
-- 1/637.5/718.64/1586.8/2103.27/2422
-- footprints in parcel-containing cells/full/outside/null-h3=
-- 1081175/1081999/824/0
-- total feature median/p99/max=1395.5/4824.17/6011; invalid=0
