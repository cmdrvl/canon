-- Three-source controlled-halo predicate-incidence audit. This extends
-- appendix_d_stratified_halo.sql with release-pinned Overture buildings while
-- preserving source lineage. Overture observations are not counted as an
-- independent vote merely because they occupy another warehouse row: most
-- NYC Overture buildings declare OpenStreetMap lineage.
--
-- The dedicated OVERTURE_MAPS_FEATURE_H3_COVERAGE projection was still empty
-- on 2026-08-30, and the typed OVERTURE_MAPS_BUILDINGS_HOT view failed because
-- its declared and produced column counts differed. This query therefore uses
-- the described OVERTURE_MAPS_FEATURES_HOT base contract and its populated H3
-- anchors. Do not treat that bypass as proof that either upstream defect is
-- repaired.
--
-- H3 blocks and assigns ownership only. Majority edges use computed geometry
-- area in both numerator and denominator. The complete parcel snapshot is an
-- audit oracle for candidate reach, never a monolithic solver input. The
-- reported components contain parcel nodes plus center-owned footprint
-- observations from two sources; they are predicate-incidence components, not
-- final solver widths and not deduplicated latent-building counts.
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
), observation_index AS (
  SELECT 'nyc_footprint' AS source_name,
         TO_VARCHAR(OBJECTID) AS observation_id,
         GEOM_GEOG AS geom,
         BBOX_XMIN,
         BBOX_YMIN,
         BBOX_XMAX,
         BBOX_YMAX,
         NULLIF(ST_AREA(GEOM_GEOG), 0) AS computed_area_m2,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 8) AS h3_r8,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 9) AS h3_r9,
         FALSE AS has_osm_lineage
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE RELEASE_DT = '2026-08-09'
    AND IS_ACTIVE_FOOTPRINT = TRUE
    AND OBJECTID IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_LON IS NOT NULL
    AND CENTROID_LAT IS NOT NULL
  UNION ALL
  SELECT 'overture_building' AS source_name,
         PROVIDER_FEATURE_ID AS observation_id,
         GEOM_GEOG AS geom,
         BBOX_XMIN,
         BBOX_YMIN,
         BBOX_XMAX,
         BBOX_YMAX,
         NULLIF(ST_AREA(GEOM_GEOG), 0) AS computed_area_m2,
         H3_R8 AS h3_r8,
         H3_POINT_TO_CELL_STRING(ST_MAKEPOINT(CENTROID_LON, CENTROID_LAT), 9) AS h3_r9,
         COALESCE(SOURCES_JSON LIKE '%\"dataset\":\"OpenStreetMap\"%', FALSE)
           AS has_osm_lineage
  FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT
  WHERE RELEASE = '2026-07-22.0'
    AND RELEASE_DT = '2026-07-22'
    AND COUNTRY = 'US'
    AND STATE = 'US-NY'
    AND DATASET = 'buildings'
    AND FEATURE_TYPE = 'building'
    AND LICENSE_CLASS = 'odbl'
    AND H3_KEY_STATUS = 'valid'
    AND PROVIDER_FEATURE_ID IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_LON IS NOT NULL
    AND CENTROID_LAT IS NOT NULL
), target_observations AS (
  SELECT s.stratum,
         s.resolution,
         s.center_cell,
         o.source_name,
         o.observation_id,
         o.geom,
         o.BBOX_XMIN,
         o.BBOX_YMIN,
         o.BBOX_XMAX,
         o.BBOX_YMAX,
         o.computed_area_m2,
         o.has_osm_lineage
  FROM strata s
  JOIN observation_index o
    ON IFF(s.resolution = 8, o.h3_r8, o.h3_r9) = s.center_cell
), work_unit_parcels AS (
  SELECT w.stratum, p.parcel_id
  FROM work_cells w
  JOIN parcel_index p
    ON IFF(w.resolution = 8, p.h3_r8, p.h3_r9) = w.work_cell
  GROUP BY w.stratum, p.parcel_id
), work_unit_observations AS (
  SELECT w.stratum, o.source_name, o.observation_id
  FROM work_cells w
  JOIN observation_index o
    ON IFF(w.resolution = 8, o.h3_r8, o.h3_r9) = w.work_cell
  GROUP BY w.stratum, o.source_name, o.observation_id
), majority_pairs AS (
  SELECT o.stratum,
         o.source_name,
         o.observation_id,
         p.parcel_id,
         IFF(IFF(o.resolution = 8, p.h3_r8, p.h3_r9) = o.center_cell,
             TRUE, FALSE) AS in_same_cell,
         IFF(w.work_cell IS NULL, FALSE, TRUE) AS in_k1
  FROM target_observations o
  JOIN parcel_index p
    ON p.BBOX_XMAX >= o.BBOX_XMIN
   AND p.BBOX_XMIN <= o.BBOX_XMAX
   AND p.BBOX_YMAX >= o.BBOX_YMIN
   AND p.BBOX_YMIN <= o.BBOX_YMAX
   AND ST_INTERSECTS(o.geom, p.geom)
  LEFT JOIN work_cells w
    ON w.stratum = o.stratum
   AND w.work_cell = IFF(o.resolution = 8, p.h3_r8, p.h3_r9)
  WHERE o.computed_area_m2 IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(o.geom, p.geom)) / o.computed_area_m2 > 0.5
), per_observation AS (
  SELECT o.stratum,
         o.source_name,
         o.observation_id,
         o.has_osm_lineage,
         COUNT(mp.parcel_id) AS global_majority_count,
         COUNT_IF(mp.in_same_cell) AS same_cell_majority_count,
         COUNT_IF(mp.in_k1) AS k1_majority_count
  FROM target_observations o
  LEFT JOIN majority_pairs mp
    ON mp.stratum = o.stratum
   AND mp.source_name = o.source_name
   AND mp.observation_id = o.observation_id
  GROUP BY o.stratum, o.source_name, o.observation_id, o.has_osm_lineage
), reach_summary AS (
  SELECT stratum,
         source_name,
         COUNT(*) AS target_observations,
         COUNT_IF(has_osm_lineage) AS osm_lineage_observations,
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
  FROM per_observation
  GROUP BY stratum, source_name
), parcel_components AS (
  SELECT p.stratum,
         1 + COUNT(DISTINCT mp.source_name || ':' || mp.observation_id)
           AS component_size
  FROM work_unit_parcels p
  LEFT JOIN majority_pairs mp
    ON mp.stratum = p.stratum
   AND mp.parcel_id = p.parcel_id
   AND mp.in_k1
  GROUP BY p.stratum, p.parcel_id
), zero_observation_components AS (
  SELECT po.stratum, 1 AS component_size
  FROM per_observation po
  WHERE po.k1_majority_count = 0
), components AS (
  SELECT * FROM parcel_components
  UNION ALL
  SELECT * FROM zero_observation_components
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
), work_parcel_counts AS (
  SELECT stratum, COUNT(*) AS work_parcels
  FROM work_unit_parcels
  GROUP BY stratum
), work_observation_counts AS (
  SELECT stratum,
         COUNT(*) AS work_observations,
         COUNT_IF(source_name = 'nyc_footprint') AS work_nyc_footprints,
         COUNT_IF(source_name = 'overture_building') AS work_overture_buildings
  FROM work_unit_observations
  GROUP BY stratum
), target_counts AS (
  SELECT stratum, COUNT(*) AS all_target_observations
  FROM per_observation
  GROUP BY stratum
), source_counts AS (
  SELECT stratum, COUNT(DISTINCT source_name) AS source_count
  FROM per_observation
  GROUP BY stratum
)
SELECT s.stratum,
       s.resolution,
       s.center_cell,
       rs.source_name,
       pc.work_parcels,
       oc.work_nyc_footprints,
       oc.work_overture_buildings,
       pc.work_parcels + oc.work_observations AS work_unit_nodes,
       rs.target_observations,
       rs.osm_lineage_observations,
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
       IFF(pc.work_parcels > 0
           AND oc.work_nyc_footprints > 0
           AND oc.work_overture_buildings > 0,
           'PASS', 'FAIL') AS work_unit_sanity,
       IFF(rs.same_one + rs.same_zero + rs.same_multi = rs.target_observations,
           'PASS', 'FAIL') AS same_denominator_sanity,
       IFF(rs.k1_one + rs.k1_zero + rs.k1_multi = rs.target_observations,
           'PASS', 'FAIL') AS k1_denominator_sanity,
       IFF(rs.global_one + rs.global_zero + rs.global_multi = rs.target_observations,
           'PASS', 'FAIL') AS global_denominator_sanity,
       IFF(rs.truth_outside_k1 = 0, 'PASS', 'FAIL') AS reach_sanity,
       IFF(rs.k1_multi = 0, 'PASS', 'FAIL') AS source_forest_sanity,
       IFF(sc.source_count = 2, 'PASS', 'FAIL') AS source_count_sanity,
       IFF(cs.component_nodes = pc.work_parcels + tc.all_target_observations,
           'PASS', 'FAIL') AS component_accounting_sanity
FROM strata s
JOIN reach_summary rs ON rs.stratum = s.stratum
JOIN work_parcel_counts pc ON pc.stratum = s.stratum
JOIN work_observation_counts oc ON oc.stratum = s.stratum
JOIN target_counts tc ON tc.stratum = s.stratum
JOIN source_counts sc ON sc.stratum = s.stratum
JOIN component_stats cs ON cs.stratum = s.stratum
ORDER BY s.resolution, s.stratum, rs.source_name;

-- Fresh 2026-08-30 expected result under the three pins above:
-- - 24 nonzero rows and all eight sanity columns PASS;
-- - Overture r8 target total 6,018: same-cell 5,917/101/0,
--   k1 and complete reference 6,005/13/0, with 88 repaired by k1;
-- - Overture r9 target total 1,401: same-cell 1,334/67/0,
--   k1 and complete reference 1,400/1/0, with 66 repaired by k1;
-- - truth_outside_k1 = 0 for every source-stratum row;
-- - Overture OSM-lineage targets total 5,967 at r8 and 1,393 at r9;
-- - combined raw-observation work units span 3,709..38,667 nodes at r8 and
--   596..7,015 at r9; component maxima span 7..128 and 5..118 respectively.
