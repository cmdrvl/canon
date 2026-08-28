-- Candidate-reach correction for the same target footprints. All parcels in
-- the pinned release remain eligible behind a complete bbox prefilter; exact
-- ST_INTERSECTS and computed-area majority still decide the edge.
WITH cells AS (
  SELECT * FROM VALUES
    ('BX_LOWER', '882a100f4dfffff'),
    ('MN_DENSE', '882a100d8bfffff')
    AS c(cell_name, h3_cell)
), footprints AS (
  SELECT c.cell_name, c.h3_cell,
         TO_VARCHAR(f.OBJECTID) AS footprint_id,
         f.GEOM_GEOG AS geom,
         f.BBOX_XMIN, f.BBOX_YMIN, f.BBOX_XMAX, f.BBOX_YMAX,
         NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS computed_area_m2
  FROM cells c
  JOIN SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
    ON f.RELEASE_DT = '2026-08-09'
   AND f.IS_ACTIVE_FOOTPRINT = TRUE
   AND f.H3_R8 = c.h3_cell
  WHERE f.OBJECTID IS NOT NULL AND f.GEOM_GEOG IS NOT NULL
), parcels AS (
  SELECT p.BBL AS parcel_id, H3_INT_TO_STRING(p.H3_R8) AS parcel_home_cell,
         p.GEOM_GEOG AS geom,
         p.BBOX_XMIN, p.BBOX_YMIN, p.BBOX_XMAX, p.BBOX_YMAX
  FROM SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE p.RELEASE = '26v1'
    AND p.RELEASE_DT = '2026-05-01'
    AND p.BBL IS NOT NULL
    AND p.GEOM_GEOG IS NOT NULL
), majority_pairs AS (
  SELECT f.cell_name, f.h3_cell, f.footprint_id,
         p.parcel_id, p.parcel_home_cell
  FROM footprints f
  JOIN parcels p
    ON p.BBOX_XMAX >= f.BBOX_XMIN
   AND p.BBOX_XMIN <= f.BBOX_XMAX
   AND p.BBOX_YMAX >= f.BBOX_YMIN
   AND p.BBOX_YMIN <= f.BBOX_YMAX
   AND ST_INTERSECTS(f.geom, p.geom)
  WHERE f.computed_area_m2 IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.computed_area_m2 > 0.5
), per_footprint AS (
  SELECT f.cell_name, f.footprint_id,
         COUNT(mp.parcel_id) AS global_majority_count,
         COUNT_IF(mp.parcel_home_cell = f.h3_cell) AS same_home_cell_majority_count,
         COUNT_IF(mp.parcel_home_cell <> f.h3_cell) AS cross_home_cell_majority_count
  FROM footprints f
  LEFT JOIN majority_pairs mp
    ON mp.cell_name = f.cell_name AND mp.footprint_id = f.footprint_id
  GROUP BY f.cell_name, f.footprint_id
)
SELECT cell_name,
       COUNT(*) AS footprint_count,
       COUNT_IF(global_majority_count = 0) AS global_majority_zero,
       COUNT_IF(global_majority_count = 1) AS global_majority_one,
       COUNT_IF(global_majority_count > 1) AS global_majority_multi,
       COUNT_IF(same_home_cell_majority_count = 0) AS same_home_cell_zero,
       COUNT_IF(cross_home_cell_majority_count > 0) AS repaired_by_cross_home_cell_parcel,
       IFF(global_majority_zero + global_majority_one + global_majority_multi = footprint_count,
           'PASS', 'FAIL') AS denominator_sanity
FROM per_footprint
GROUP BY cell_name
ORDER BY cell_name;

-- Expected 2026-08-28 global majority one/zero/multi and repaired:
-- BX_LOWER 290/1/0, repaired=3; MN_DENSE 2352/2/0, repaired=20.
