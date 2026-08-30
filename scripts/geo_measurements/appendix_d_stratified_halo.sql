-- Stratified controlled-halo audit over the source-bound MapPLUTO geom-v3
-- plane. Run list_tables and describe_table first. The twelve center-plus-k1
-- disks below were emitted by `canon geo tile-work`, not by warehouse neighbor
-- helpers. Snowflake point-to-cell is used only to assign release-pinned
-- representative points to those explicit cells; the previously bad control
-- point (-73.977264, 40.753429) must first return h3o's
-- `892a100d62bffff` at r9.
-- Run appendix_d_stratified_halo_centers.sql next and require its six r9
-- centers to match the declared r9 strata below. H3 ancestry is logically
-- exact but geometrically approximate across resolutions, so r9 target counts
-- use the complete independently point-binned r9 populations exposed there,
-- not only the subset whose points independently bin to the parent r8 stratum.
--
-- The complete bbox reference is an audit oracle. It is never a proposal to
-- solve the full parcel snapshot monolithically. Component statistics are for
-- the parcel/center-footprint graph induced by geometric-area-majority edges.
-- They are not the final solver's constraint-incidence widths: later evidence
-- may couple otherwise isolated parcel variables.
WITH strata AS (
  SELECT * FROM VALUES
    ('BK_DENSE_R8', 8, '882a100d8bfffff'),
    ('BX_LOWER_R8', 8, '882a100f4dfffff'),
    ('MN_SMALL_R8', 8, '882a1008c7fffff'),
    ('QN_DENSE_R8', 8, '882a103b6bfffff'),
    ('QN_MEDIUM_R8', 8, '882a100e25fffff'),
    ('SI_LOW_R8', 8, '882a106019fffff'),
    ('BK_DENSE_R9', 9, '892a100d8a3ffff'),
    ('BX_LOWER_R9', 9, '892a100f4c3ffff'),
    ('MN_SMALL_R9', 9, '892a1008c67ffff'),
    ('QN_DENSE_R9', 9, '892a103b6b7ffff'),
    ('QN_MEDIUM_R9', 9, '892a100e24fffff'),
    ('SI_LOW_R9', 9, '892a1060197ffff')
    AS s(stratum, resolution, center_cell)
), work_cells AS (
  SELECT * FROM VALUES
    ('BK_DENSE_R8', 8, '882a100d81fffff'),
    ('BK_DENSE_R8', 8, '882a100d83fffff'),
    ('BK_DENSE_R8', 8, '882a100d89fffff'),
    ('BK_DENSE_R8', 8, '882a100d8bfffff'),
    ('BK_DENSE_R8', 8, '882a100d9dfffff'),
    ('BK_DENSE_R8', 8, '882a100dd5fffff'),
    ('BK_DENSE_R8', 8, '882a100dd7fffff'),
    ('BX_LOWER_R8', 8, '882a1001a7fffff'),
    ('BX_LOWER_R8', 8, '882a100a93fffff'),
    ('BX_LOWER_R8', 8, '882a100a9bfffff'),
    ('BX_LOWER_R8', 8, '882a100f41fffff'),
    ('BX_LOWER_R8', 8, '882a100f45fffff'),
    ('BX_LOWER_R8', 8, '882a100f49fffff'),
    ('BX_LOWER_R8', 8, '882a100f4dfffff'),
    ('MN_SMALL_R8', 8, '882a100889fffff'),
    ('MN_SMALL_R8', 8, '882a10088dfffff'),
    ('MN_SMALL_R8', 8, '882a1008c1fffff'),
    ('MN_SMALL_R8', 8, '882a1008c3fffff'),
    ('MN_SMALL_R8', 8, '882a1008c5fffff'),
    ('MN_SMALL_R8', 8, '882a1008c7fffff'),
    ('MN_SMALL_R8', 8, '882a1008ebfffff'),
    ('QN_DENSE_R8', 8, '882a103b0dfffff'),
    ('QN_DENSE_R8', 8, '882a103b45fffff'),
    ('QN_DENSE_R8', 8, '882a103b47fffff'),
    ('QN_DENSE_R8', 8, '882a103b61fffff'),
    ('QN_DENSE_R8', 8, '882a103b63fffff'),
    ('QN_DENSE_R8', 8, '882a103b69fffff'),
    ('QN_DENSE_R8', 8, '882a103b6bfffff'),
    ('QN_MEDIUM_R8', 8, '882a100e21fffff'),
    ('QN_MEDIUM_R8', 8, '882a100e25fffff'),
    ('QN_MEDIUM_R8', 8, '882a100e27fffff'),
    ('QN_MEDIUM_R8', 8, '882a100e2dfffff'),
    ('QN_MEDIUM_R8', 8, '882a100f19fffff'),
    ('QN_MEDIUM_R8', 8, '882a100f1bfffff'),
    ('QN_MEDIUM_R8', 8, '882a100f53fffff'),
    ('SI_LOW_R8', 8, '882a106011fffff'),
    ('SI_LOW_R8', 8, '882a106019fffff'),
    ('SI_LOW_R8', 8, '882a10601bfffff'),
    ('SI_LOW_R8', 8, '882a10601dfffff'),
    ('SI_LOW_R8', 8, '882a106053fffff'),
    ('SI_LOW_R8', 8, '882a106057fffff'),
    ('SI_LOW_R8', 8, '882a1062a5fffff'),
    ('BK_DENSE_R9', 9, '892a100d8a3ffff'),
    ('BK_DENSE_R9', 9, '892a100d8a7ffff'),
    ('BK_DENSE_R9', 9, '892a100d8abffff'),
    ('BK_DENSE_R9', 9, '892a100d8afffff'),
    ('BK_DENSE_R9', 9, '892a100d8b3ffff'),
    ('BK_DENSE_R9', 9, '892a100d8b7ffff'),
    ('BK_DENSE_R9', 9, '892a100d8bbffff'),
    ('BX_LOWER_R9', 9, '892a100f4c3ffff'),
    ('BX_LOWER_R9', 9, '892a100f4c7ffff'),
    ('BX_LOWER_R9', 9, '892a100f4cbffff'),
    ('BX_LOWER_R9', 9, '892a100f4cfffff'),
    ('BX_LOWER_R9', 9, '892a100f4d3ffff'),
    ('BX_LOWER_R9', 9, '892a100f4d7ffff'),
    ('BX_LOWER_R9', 9, '892a100f4dbffff'),
    ('MN_SMALL_R9', 9, '892a1008893ffff'),
    ('MN_SMALL_R9', 9, '892a100889bffff'),
    ('MN_SMALL_R9', 9, '892a1008c2bffff'),
    ('MN_SMALL_R9', 9, '892a1008c63ffff'),
    ('MN_SMALL_R9', 9, '892a1008c67ffff'),
    ('MN_SMALL_R9', 9, '892a1008c6fffff'),
    ('MN_SMALL_R9', 9, '892a1008c77ffff'),
    ('QN_DENSE_R9', 9, '892a103b44fffff'),
    ('QN_DENSE_R9', 9, '892a103b46bffff'),
    ('QN_DENSE_R9', 9, '892a103b47bffff'),
    ('QN_DENSE_R9', 9, '892a103b6a3ffff'),
    ('QN_DENSE_R9', 9, '892a103b6a7ffff'),
    ('QN_DENSE_R9', 9, '892a103b6b3ffff'),
    ('QN_DENSE_R9', 9, '892a103b6b7ffff'),
    ('QN_MEDIUM_R9', 9, '892a100e243ffff'),
    ('QN_MEDIUM_R9', 9, '892a100e247ffff'),
    ('QN_MEDIUM_R9', 9, '892a100e24bffff'),
    ('QN_MEDIUM_R9', 9, '892a100e24fffff'),
    ('QN_MEDIUM_R9', 9, '892a100e27bffff'),
    ('QN_MEDIUM_R9', 9, '892a100f1b3ffff'),
    ('QN_MEDIUM_R9', 9, '892a100f1b7ffff'),
    ('SI_LOW_R9', 9, '892a1060183ffff'),
    ('SI_LOW_R9', 9, '892a1060187ffff'),
    ('SI_LOW_R9', 9, '892a1060193ffff'),
    ('SI_LOW_R9', 9, '892a1060197ffff'),
    ('SI_LOW_R9', 9, '892a106052fffff'),
    ('SI_LOW_R9', 9, '892a1062a4bffff'),
    ('SI_LOW_R9', 9, '892a1062a5bffff')
    AS w(stratum, resolution, work_cell)
), footprint_index AS (
  SELECT TO_VARCHAR(OBJECTID) AS footprint_id,
         GEOM_GEOG AS geom,
         BBOX_XMIN,
         BBOX_YMIN,
         BBOX_XMAX,
         BBOX_YMAX,
         NULLIF(ST_AREA(GEOM_GEOG), 0) AS computed_area_m2,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 8) AS h3_r8,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 9) AS h3_r9
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE RELEASE_DT = '2026-08-09'
    AND IS_ACTIVE_FOOTPRINT = TRUE
    AND OBJECTID IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_LON IS NOT NULL
    AND CENTROID_LAT IS NOT NULL
), parcel_index AS (
  SELECT BBL AS parcel_id,
         TO_GEOGRAPHY(GEOM_WKT) AS geom,
         BBOX_XMIN,
         BBOX_YMIN,
         BBOX_XMAX,
         BBOX_YMAX,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 8) AS h3_r8,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 9) AS h3_r9
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT
  WHERE RELEASE = '26v2'
    AND RELEASE_DT = '2026-08-01'
    AND BBL IS NOT NULL
    AND GEOM_WKT IS NOT NULL
    AND CENTROID_LON IS NOT NULL
    AND CENTROID_LAT IS NOT NULL
), target_footprints AS (
  SELECT s.stratum,
         s.resolution,
         s.center_cell,
         f.footprint_id,
         f.geom,
         f.BBOX_XMIN,
         f.BBOX_YMIN,
         f.BBOX_XMAX,
         f.BBOX_YMAX,
         f.computed_area_m2
  FROM strata s
  JOIN footprint_index f
    ON IFF(s.resolution = 8, f.h3_r8, f.h3_r9) = s.center_cell
), work_unit_parcels AS (
  SELECT w.stratum, p.parcel_id
  FROM work_cells w
  JOIN parcel_index p
    ON IFF(w.resolution = 8, p.h3_r8, p.h3_r9) = w.work_cell
  GROUP BY w.stratum, p.parcel_id
), work_unit_footprints AS (
  SELECT w.stratum, f.footprint_id
  FROM work_cells w
  JOIN footprint_index f
    ON IFF(w.resolution = 8, f.h3_r8, f.h3_r9) = w.work_cell
  GROUP BY w.stratum, f.footprint_id
), majority_pairs AS (
  SELECT f.stratum,
         f.footprint_id,
         p.parcel_id,
         IFF(IFF(f.resolution = 8, p.h3_r8, p.h3_r9) = f.center_cell,
             TRUE, FALSE) AS in_same_cell,
         IFF(w.work_cell IS NULL, FALSE, TRUE) AS in_k1
  FROM target_footprints f
  JOIN parcel_index p
    ON p.BBOX_XMAX >= f.BBOX_XMIN
   AND p.BBOX_XMIN <= f.BBOX_XMAX
   AND p.BBOX_YMAX >= f.BBOX_YMIN
   AND p.BBOX_YMIN <= f.BBOX_YMAX
   AND ST_INTERSECTS(f.geom, p.geom)
  LEFT JOIN work_cells w
    ON w.stratum = f.stratum
   AND w.work_cell = IFF(f.resolution = 8, p.h3_r8, p.h3_r9)
  WHERE f.computed_area_m2 IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.computed_area_m2 > 0.5
), per_footprint AS (
  SELECT f.stratum,
         f.footprint_id,
         COUNT(mp.parcel_id) AS global_majority_count,
         COUNT_IF(mp.in_same_cell) AS same_cell_majority_count,
         COUNT_IF(mp.in_k1) AS k1_majority_count
  FROM target_footprints f
  LEFT JOIN majority_pairs mp
    ON mp.stratum = f.stratum
   AND mp.footprint_id = f.footprint_id
  GROUP BY f.stratum, f.footprint_id
), reach_summary AS (
  SELECT stratum,
         COUNT(*) AS target_footprints,
         COUNT_IF(same_cell_majority_count = 1) AS same_one,
         COUNT_IF(same_cell_majority_count = 0) AS same_zero,
         COUNT_IF(same_cell_majority_count > 1) AS same_multi,
         COUNT_IF(k1_majority_count = 1) AS k1_one,
         COUNT_IF(k1_majority_count = 0) AS k1_zero,
         COUNT_IF(k1_majority_count > 1) AS k1_multi,
         COUNT_IF(global_majority_count = 1) AS global_one,
         COUNT_IF(global_majority_count = 0) AS global_zero,
         COUNT_IF(global_majority_count > 1) AS global_multi,
         COUNT_IF(global_majority_count = 1 AND k1_majority_count = 0)
           AS truth_outside_k1,
         COUNT_IF(global_majority_count = 1
                  AND same_cell_majority_count = 0
                  AND k1_majority_count = 1) AS repaired_by_k1
  FROM per_footprint
  GROUP BY stratum
), parcel_components AS (
  SELECT p.stratum,
         1 + COUNT(DISTINCT mp.footprint_id) AS component_size
  FROM work_unit_parcels p
  LEFT JOIN majority_pairs mp
    ON mp.stratum = p.stratum
   AND mp.parcel_id = p.parcel_id
   AND mp.in_k1
  GROUP BY p.stratum, p.parcel_id
), zero_footprint_components AS (
  SELECT pf.stratum, 1 AS component_size
  FROM per_footprint pf
  WHERE pf.k1_majority_count = 0
), components AS (
  SELECT * FROM parcel_components
  UNION ALL
  SELECT * FROM zero_footprint_components
), component_stats AS (
  SELECT stratum,
         COUNT(*) AS component_count,
         SUM(component_size) AS component_nodes,
         ROUND(AVG(component_size), 3) AS mean_component_size,
         MEDIAN(component_size) AS median_component_size,
         APPROX_PERCENTILE(component_size, 0.9) AS p90_component_size,
         MAX(component_size) AS max_component_size
  FROM components
  GROUP BY stratum
), component_hist AS (
  SELECT stratum,
         LISTAGG(TO_VARCHAR(component_size) || ':' || TO_VARCHAR(component_count), ', ')
           WITHIN GROUP (ORDER BY component_size) AS component_size_histogram
  FROM (
    SELECT stratum, component_size, COUNT(*) AS component_count
    FROM components
    GROUP BY stratum, component_size
  )
  GROUP BY stratum
), work_counts AS (
  SELECT s.stratum,
         COUNT(DISTINCT p.parcel_id) AS work_parcels,
         COUNT(DISTINCT f.footprint_id) AS work_footprints
  FROM strata s
  LEFT JOIN work_unit_parcels p ON p.stratum = s.stratum
  LEFT JOIN work_unit_footprints f ON f.stratum = s.stratum
  GROUP BY s.stratum
)
SELECT s.stratum,
       s.resolution,
       s.center_cell,
       wc.work_parcels,
       wc.work_footprints,
       wc.work_parcels + wc.work_footprints AS work_unit_nodes,
       rs.target_footprints,
       rs.same_one,
       rs.same_zero,
       rs.same_multi,
       rs.k1_one,
       rs.k1_zero,
       rs.k1_multi,
       rs.global_one,
       rs.global_zero,
       rs.global_multi,
       rs.truth_outside_k1,
       rs.repaired_by_k1,
       cs.component_count,
       cs.component_nodes,
       cs.mean_component_size,
       cs.median_component_size,
       cs.p90_component_size,
       cs.max_component_size,
       ch.component_size_histogram,
       IFF(wc.work_parcels > 0 AND wc.work_footprints > 0,
           'PASS', 'FAIL') AS work_unit_sanity,
       IFF(rs.same_one + rs.same_zero + rs.same_multi = rs.target_footprints,
           'PASS', 'FAIL') AS same_denominator_sanity,
       IFF(rs.k1_one + rs.k1_zero + rs.k1_multi = rs.target_footprints,
           'PASS', 'FAIL') AS k1_denominator_sanity,
       IFF(rs.global_one + rs.global_zero + rs.global_multi = rs.target_footprints,
           'PASS', 'FAIL') AS global_denominator_sanity,
       IFF(rs.truth_outside_k1 = 0, 'PASS', 'FAIL') AS reach_sanity,
       IFF(rs.k1_multi = 0, 'PASS', 'FAIL') AS forest_sanity,
       IFF(cs.component_nodes = wc.work_parcels + rs.target_footprints,
           'PASS', 'FAIL') AS component_accounting_sanity
FROM strata s
JOIN work_counts wc ON wc.stratum = s.stratum
JOIN reach_summary rs ON rs.stratum = s.stratum
JOIN component_stats cs ON cs.stratum = s.stratum
JOIN component_hist ch ON ch.stratum = s.stratum
ORDER BY s.resolution, s.stratum;

-- Fresh 2026-08-30 expected result under MapPLUTO 26v2 / 2026-08-01 and
-- NYC footprints 2026-08-09:
-- - all seven sanity columns PASS in all twelve rows;
-- - truth_outside_k1 = 0 and global_multi = k1_multi = 0 in all rows;
-- - r8 target total 6,002: same-cell 5,895/107, k1/global 5,995/7;
-- - r9 target total 1,419: same-cell 1,344/75, k1/global 1,418/1;
-- - r8 work-unit nodes range 2,260..25,786; component max range 4..71;
-- - r9 work-unit nodes range 378..4,670; component max range 3..65.
