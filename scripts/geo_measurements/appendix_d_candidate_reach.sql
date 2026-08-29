-- Controlled-halo candidate-reach audit for the same target footprints.
-- `work_cells` is the explicit h3o r8+k1 disk emitted by Canon tile-work. The
-- complete bbox reference is an audit oracle over the pinned parcel snapshot,
-- not a proposed monolithic solve. Snowflake GEOGRAPHY predicates decide this
-- empirical reference; they are not Canon's exact local integer predicates.
WITH cells AS (
  SELECT * FROM VALUES
    ('BX_LOWER', '882a100f4dfffff'),
    ('BK_DENSE', '882a100d8bfffff')
    AS c(cell_name, h3_cell)
), work_cells AS (
  SELECT * FROM VALUES
    ('BX_LOWER', '882a1001a7fffff'),
    ('BX_LOWER', '882a100a93fffff'),
    ('BX_LOWER', '882a100a9bfffff'),
    ('BX_LOWER', '882a100f41fffff'),
    ('BX_LOWER', '882a100f45fffff'),
    ('BX_LOWER', '882a100f49fffff'),
    ('BX_LOWER', '882a100f4dfffff'),
    ('BK_DENSE', '882a100d81fffff'),
    ('BK_DENSE', '882a100d83fffff'),
    ('BK_DENSE', '882a100d89fffff'),
    ('BK_DENSE', '882a100d8bfffff'),
    ('BK_DENSE', '882a100d9dfffff'),
    ('BK_DENSE', '882a100dd5fffff'),
    ('BK_DENSE', '882a100dd7fffff')
    AS w(cell_name, work_cell)
), footprints AS (
  SELECT c.cell_name, c.h3_cell,
         TO_VARCHAR(f.OBJECTID) AS footprint_id,
         f.GEOM_GEOG AS geom,
         f.BBOX_XMIN, f.BBOX_YMIN, f.BBOX_XMAX, f.BBOX_YMAX,
         NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS computed_area_m2
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
    ON f.RELEASE_DT = '2026-08-09'
   AND f.IS_ACTIVE_FOOTPRINT = TRUE
   AND f.H3_R8 = c.h3_cell
  WHERE f.OBJECTID IS NOT NULL AND f.GEOM_GEOG IS NOT NULL
), parcels AS (
  SELECT v.BBL AS parcel_id, H3_INT_TO_STRING(h.H3_R8) AS parcel_home_cell,
         TO_GEOGRAPHY(v.GEOM_WKT) AS geom,
         v.BBOX_XMIN, v.BBOX_YMIN, v.BBOX_XMAX, v.BBOX_YMAX
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT v
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT h
    ON h.RELEASE = v.RELEASE
   AND h.RELEASE_DT = v.RELEASE_DT
   AND h.BBL = v.BBL
  WHERE v.RELEASE = '26v1'
    AND v.RELEASE_DT = '2026-05-01'
    AND v.BBL IS NOT NULL
    AND v.GEOM_WKT IS NOT NULL
    AND h.H3_R8 IS NOT NULL
), majority_pairs AS (
  SELECT f.cell_name, f.h3_cell, f.footprint_id,
         p.parcel_id, p.parcel_home_cell,
         IFF(w.work_cell IS NULL, FALSE, TRUE) AS in_k1
  FROM footprints f
  JOIN parcels p
    ON p.BBOX_XMAX >= f.BBOX_XMIN
   AND p.BBOX_XMIN <= f.BBOX_XMAX
   AND p.BBOX_YMAX >= f.BBOX_YMIN
   AND p.BBOX_YMIN <= f.BBOX_YMAX
   AND ST_INTERSECTS(f.geom, p.geom)
  LEFT JOIN work_cells w
    ON w.cell_name = f.cell_name AND w.work_cell = p.parcel_home_cell
  WHERE f.computed_area_m2 IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.computed_area_m2 > 0.5
), per_footprint AS (
  SELECT f.cell_name, f.footprint_id,
         COUNT(mp.parcel_id) AS global_majority_count,
         COUNT_IF(mp.parcel_home_cell = f.h3_cell) AS same_home_cell_majority_count,
         COUNT_IF(mp.in_k1) AS k1_majority_count
  FROM footprints f
  LEFT JOIN majority_pairs mp
    ON mp.cell_name = f.cell_name AND mp.footprint_id = f.footprint_id
  GROUP BY f.cell_name, f.footprint_id
)
SELECT cell_name,
       COUNT(*) AS footprint_count,
       COUNT_IF(same_home_cell_majority_count = 1) AS same_cell_one,
       COUNT_IF(same_home_cell_majority_count = 0) AS same_cell_zero,
       COUNT_IF(k1_majority_count = 1) AS k1_one,
       COUNT_IF(k1_majority_count = 0) AS k1_zero,
       COUNT_IF(global_majority_count = 1) AS global_one,
       COUNT_IF(global_majority_count = 0) AS global_zero,
       COUNT_IF(global_majority_count > 1) AS global_multi,
       COUNT_IF(global_majority_count = 1 AND k1_majority_count = 0) AS truth_outside_k1,
       COUNT_IF(global_majority_count = 1
                    AND same_home_cell_majority_count = 0
                    AND k1_majority_count = 1) AS repaired_by_k1,
       IFF(global_one + global_zero + global_multi = footprint_count,
           'PASS', 'FAIL') AS denominator_sanity
FROM per_footprint
GROUP BY cell_name
ORDER BY cell_name;

-- Fresh 2026-08-29 geom-v3 expected same / k1 / global one-zero-multi:
-- BX_LOWER 287/4, 290/1, 290/1/0; outside k1=0, repaired by k1=3.
-- BK_DENSE 2333/21, 2353/1, 2353/1/0; outside k1=0, repaired by k1=20.
-- The older HOT-only 2026-08-28 result remains a historical receipt; geom-v3
-- changes one Brooklyn row and supersedes it for source-bound geometry work.
