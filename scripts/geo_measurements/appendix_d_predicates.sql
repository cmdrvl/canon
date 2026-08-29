-- Same-H3-home-cell reproduction of predicates A/B/C. This intentionally
-- measures the legacy candidate restriction; run appendix_d_candidate_reach.sql
-- before interpreting zero-majority rows as geometric residuals.
WITH cells AS (
  SELECT * FROM VALUES
    ('BX_LOWER', '882a100f4dfffff'),
    ('BK_DENSE', '882a100d8bfffff')
    AS c(cell_name, h3_cell)
), parcels AS (
  SELECT c.cell_name, p.BBL AS parcel_id, p.GEOM_GEOG AS geom
  FROM cells c
  JOIN SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.RELEASE = '26v1'
   AND p.RELEASE_DT = '2026-05-01'
   AND p.H3_R8 = H3_STRING_TO_INT(c.h3_cell)
  WHERE p.BBL IS NOT NULL AND p.GEOM_GEOG IS NOT NULL
), footprints AS (
  SELECT c.cell_name, TO_VARCHAR(f.OBJECTID) AS footprint_id,
         f.GEOM_GEOG AS geom,
         NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS computed_area_m2
  FROM cells c
  JOIN SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
    ON f.RELEASE_DT = '2026-08-09'
   AND f.IS_ACTIVE_FOOTPRINT = TRUE
   AND f.H3_R8 = c.h3_cell
  WHERE f.OBJECTID IS NOT NULL AND f.GEOM_GEOG IS NOT NULL
), intersecting_pairs AS (
  SELECT f.cell_name, f.footprint_id, p.parcel_id,
         ST_CONTAINS(p.geom, f.geom) AS parcel_contains_footprint,
         ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.computed_area_m2
           AS computed_overlap_fraction
  FROM footprints f
  JOIN parcels p
    ON p.cell_name = f.cell_name AND ST_INTERSECTS(f.geom, p.geom)
  WHERE f.computed_area_m2 IS NOT NULL
), per_footprint AS (
  SELECT f.cell_name, f.footprint_id,
         COUNT(ip.parcel_id) AS intersects_count,
         COUNT_IF(ip.parcel_contains_footprint) AS contains_count,
         COUNT_IF(ip.computed_overlap_fraction > 0.5) AS majority_count
  FROM footprints f
  LEFT JOIN intersecting_pairs ip
    ON ip.cell_name = f.cell_name AND ip.footprint_id = f.footprint_id
  GROUP BY f.cell_name, f.footprint_id
), parcel_overlap_pairs AS (
  SELECT p1.cell_name, p1.parcel_id AS parcel_id_1, p2.parcel_id AS parcel_id_2
  FROM parcels p1
  JOIN parcels p2
    ON p2.cell_name = p1.cell_name
   AND p1.parcel_id < p2.parcel_id
   AND ST_INTERSECTS(p1.geom, p2.geom)
   AND ST_AREA(ST_INTERSECTION(p1.geom, p2.geom)) > 0
), metrics AS (
  SELECT cell_name,
         COUNT(*) AS footprint_count,
         COUNT_IF(intersects_count = 0) AS intersects_zero,
         COUNT_IF(intersects_count = 1) AS intersects_one,
         COUNT_IF(intersects_count > 1) AS intersects_multi,
         COUNT_IF(contains_count = 0) AS contains_zero,
         COUNT_IF(contains_count = 1) AS contains_one,
         COUNT_IF(contains_count > 1) AS contains_multi,
         COUNT_IF(majority_count = 0) AS majority_zero,
         COUNT_IF(majority_count = 1) AS majority_one,
         COUNT_IF(majority_count > 1) AS majority_multi
  FROM per_footprint
  GROUP BY cell_name
)
SELECT m.*,
       (SELECT COUNT(*) FROM parcels p WHERE p.cell_name = m.cell_name) AS parcel_count,
       (SELECT COUNT(*) FROM parcel_overlap_pairs o WHERE o.cell_name = m.cell_name)
         AS positive_area_parcel_overlap_pairs,
       IFF(intersects_zero + intersects_one + intersects_multi = footprint_count
           AND contains_zero + contains_one + contains_multi = footprint_count
           AND majority_zero + majority_one + majority_multi = footprint_count,
           'PASS', 'FAIL') AS denominator_sanity
FROM metrics m
ORDER BY cell_name;

-- Expected 2026-08-28 majority one/zero/multi:
-- BX_LOWER 287/4/0; BK_DENSE 2332/22/0.
