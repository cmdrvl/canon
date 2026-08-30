-- Deterministically select one logical r9 child for each declared r8 stratum.
-- Selection ranks the combined release-pinned parcel plus active-footprint
-- population whose representative points independently bin to the r8 stratum,
-- with canonical H3 text as the tie-breaker. H3 logical ancestry is exact but
-- geometric containment across resolutions is approximate: the complete r9
-- population can therefore include points that independently bin to another
-- r8 cell. The final columns expose that difference rather than silently
-- changing denominators. Run this before appendix_d_stratified_halo.sql and
-- require every returned r9 cell to match that file's declared center.
WITH strata AS (
  SELECT * FROM VALUES
    ('BK_DENSE', '882a100d8bfffff'),
    ('BX_LOWER', '882a100f4dfffff'),
    ('MN_SMALL', '882a1008c7fffff'),
    ('QN_DENSE', '882a103b6bfffff'),
    ('QN_MEDIUM', '882a100e25fffff'),
    ('SI_LOW', '882a106019fffff')
    AS s(cell_name, r8_cell)
), logical_children AS (
  -- Emitted by h3o 0.10.0 through Canon's pinned dependency. This explicit
  -- relation prevents approximate geometric containment from admitting an r9
  -- cell whose logical parent is a neighboring r8 stratum.
  SELECT * FROM VALUES
    ('BK_DENSE', '882a100d8bfffff', '892a100d8a3ffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8a7ffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8abffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8afffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8b3ffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8b7ffff'),
    ('BK_DENSE', '882a100d8bfffff', '892a100d8bbffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4c3ffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4c7ffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4cbffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4cfffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4d3ffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4d7ffff'),
    ('BX_LOWER', '882a100f4dfffff', '892a100f4dbffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c63ffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c67ffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c6bffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c6fffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c73ffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c77ffff'),
    ('MN_SMALL', '882a1008c7fffff', '892a1008c7bffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6a3ffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6a7ffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6abffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6afffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6b3ffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6b7ffff'),
    ('QN_DENSE', '882a103b6bfffff', '892a103b6bbffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e243ffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e247ffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e24bffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e24fffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e253ffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e257ffff'),
    ('QN_MEDIUM', '882a100e25fffff', '892a100e25bffff'),
    ('SI_LOW', '882a106019fffff', '892a1060183ffff'),
    ('SI_LOW', '882a106019fffff', '892a1060187ffff'),
    ('SI_LOW', '882a106019fffff', '892a106018bffff'),
    ('SI_LOW', '882a106019fffff', '892a106018fffff'),
    ('SI_LOW', '882a106019fffff', '892a1060193ffff'),
    ('SI_LOW', '882a106019fffff', '892a1060197ffff'),
    ('SI_LOW', '882a106019fffff', '892a106019bffff')
    AS c(cell_name, r8_cell, r9_cell)
), footprint_cells AS (
  SELECT TO_VARCHAR(f.OBJECTID) AS footprint_id,
         H3_POINT_TO_CELL_STRING(
           ST_MAKEPOINT(f.CENTROID_LON, f.CENTROID_LAT), 8
         ) AS r8_cell,
         H3_POINT_TO_CELL_STRING(
           ST_MAKEPOINT(f.CENTROID_LON, f.CENTROID_LAT), 9
         ) AS r9_cell
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
  WHERE f.RELEASE_DT = '2026-08-09'
    AND f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.OBJECTID IS NOT NULL
    AND f.CENTROID_LON IS NOT NULL
    AND f.CENTROID_LAT IS NOT NULL
), parcel_cells AS (
  SELECT TO_VARCHAR(p.BBL) AS parcel_id,
         H3_POINT_TO_CELL_STRING(
           ST_MAKEPOINT(p.CENTROID_LON, p.CENTROID_LAT), 8
         ) AS r8_cell,
         H3_POINT_TO_CELL_STRING(
           ST_MAKEPOINT(p.CENTROID_LON, p.CENTROID_LAT), 9
         ) AS r9_cell
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT p
  WHERE p.RELEASE = '26v2'
    AND p.RELEASE_DT = '2026-08-01'
    AND p.BBL IS NOT NULL
    AND p.CENTROID_LON IS NOT NULL
    AND p.CENTROID_LAT IS NOT NULL
), footprint_r9 AS (
  SELECT s.cell_name,
         s.r8_cell,
         f.r9_cell,
         COUNT(DISTINCT f.footprint_id) AS footprint_count
  FROM strata s
  JOIN footprint_cells f ON f.r8_cell = s.r8_cell
  JOIN logical_children c
    ON c.cell_name = s.cell_name
   AND c.r8_cell = s.r8_cell
   AND c.r9_cell = f.r9_cell
  GROUP BY s.cell_name, s.r8_cell, f.r9_cell
), parcel_r9 AS (
  SELECT s.cell_name,
         s.r8_cell,
         p.r9_cell,
         COUNT(DISTINCT p.parcel_id) AS parcel_count
  FROM strata s
  JOIN parcel_cells p ON p.r8_cell = s.r8_cell
  JOIN logical_children c
    ON c.cell_name = s.cell_name
   AND c.r8_cell = s.r8_cell
   AND c.r9_cell = p.r9_cell
  GROUP BY s.cell_name, s.r8_cell, p.r9_cell
), combined AS (
  SELECT COALESCE(f.cell_name, p.cell_name) AS cell_name,
         COALESCE(f.r8_cell, p.r8_cell) AS r8_cell,
         COALESCE(f.r9_cell, p.r9_cell) AS r9_cell,
         COALESCE(p.parcel_count, 0) AS parcel_count,
         COALESCE(f.footprint_count, 0) AS footprint_count
  FROM footprint_r9 f
  FULL OUTER JOIN parcel_r9 p
    ON p.cell_name = f.cell_name
   AND p.r9_cell = f.r9_cell
), ranked AS (
  SELECT *,
         ROW_NUMBER() OVER (
           PARTITION BY cell_name
           ORDER BY parcel_count + footprint_count DESC, r9_cell
         ) AS density_rank
  FROM combined
), selected AS (
  SELECT *
  FROM ranked
  WHERE density_rank = 1
), full_r9_footprints AS (
  SELECT s.cell_name,
         COUNT(DISTINCT f.footprint_id) AS full_footprint_count,
         COUNT(DISTINCT IFF(f.r8_cell <> s.r8_cell, f.footprint_id, NULL))
           AS other_r8_footprint_count
  FROM selected s
  JOIN footprint_cells f ON f.r9_cell = s.r9_cell
  GROUP BY s.cell_name
), full_r9_parcels AS (
  SELECT s.cell_name,
         COUNT(DISTINCT p.parcel_id) AS full_parcel_count,
         COUNT(DISTINCT IFF(p.r8_cell <> s.r8_cell, p.parcel_id, NULL))
           AS other_r8_parcel_count
  FROM selected s
  JOIN parcel_cells p ON p.r9_cell = s.r9_cell
  GROUP BY s.cell_name
)
SELECT s.cell_name,
       s.r8_cell,
       s.r9_cell,
       s.parcel_count AS selection_parcels_in_r8,
       s.footprint_count AS selection_footprints_in_r8,
       s.parcel_count + s.footprint_count AS selection_combined_in_r8,
       p.full_parcel_count AS full_r9_parcels,
       f.full_footprint_count AS full_r9_footprints,
       p.other_r8_parcel_count AS parcels_assigned_other_r8,
       f.other_r8_footprint_count AS footprints_assigned_other_r8,
       IFF(p.full_parcel_count = s.parcel_count + p.other_r8_parcel_count,
           'PASS', 'FAIL') AS parcel_population_partition_sanity,
       IFF(f.full_footprint_count = s.footprint_count + f.other_r8_footprint_count,
           'PASS', 'FAIL') AS footprint_population_partition_sanity
FROM selected s
JOIN full_r9_parcels p ON p.cell_name = s.cell_name
JOIN full_r9_footprints f ON f.cell_name = s.cell_name
ORDER BY s.cell_name;

-- Fresh 2026-08-30 expected centers under the pinned releases above:
-- Counts are selection-in-r8 / complete-r9; the final pair is the independently
-- binned other-r8 population (parcels / footprints). Both population-partition
-- sanity columns must PASS:
-- BK_DENSE  892a100d8a3ffff  369/369 parcels  375/375 footprints   0/0 other-r8
-- BX_LOWER  892a100f4c3ffff   86/86 parcels    69/69 footprints   0/0 other-r8
-- MN_SMALL  892a1008c67ffff   26/36 parcels    24/34 footprints  10/10 other-r8
-- QN_DENSE  892a103b6b7ffff  266/266 parcels  362/362 footprints  0/0 other-r8
-- QN_MEDIUM 892a100e24fffff  213/233 parcels  351/386 footprints 20/35 other-r8
-- SI_LOW    892a1060197ffff   70/70 parcels   187/193 footprints  0/6 other-r8
