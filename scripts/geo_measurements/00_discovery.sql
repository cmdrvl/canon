-- Run list_tables and describe_table for all three fully qualified sources
-- before this query. This is a bounded positive control, not a row-count claim
-- about the entire tables.
WITH cells AS (
  SELECT * FROM VALUES
    ('BK_DENSE', '882a100d8bfffff'),
    ('BX_LOWER', '882a100f4dfffff'),
    ('QN_1500', '882a103b6bfffff')
    AS c(cell_name, h3_cell)
), counts AS (
  SELECT c.cell_name, c.h3_cell, 'mappluto_parcel' AS source,
         COUNT(DISTINCT NULLIF(p.BBL, '')) AS feature_count
  FROM cells c
  LEFT JOIN SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.RELEASE = '26v1'
   AND p.RELEASE_DT = '2026-05-01'
   AND p.H3_R8 = H3_STRING_TO_INT(c.h3_cell)
  GROUP BY c.cell_name, c.h3_cell
  UNION ALL
  SELECT c.cell_name, c.h3_cell, 'nyc_footprint' AS source,
         COUNT(DISTINCT f.OBJECTID) AS feature_count
  FROM cells c
  LEFT JOIN SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
    ON f.RELEASE_DT = '2026-08-09'
   AND f.IS_ACTIVE_FOOTPRINT = TRUE
   AND f.H3_R8 = c.h3_cell
  GROUP BY c.cell_name, c.h3_cell
  UNION ALL
  SELECT c.cell_name, c.h3_cell, 'fema_structure' AS source,
         COUNT(DISTINCT f.PROVIDER_FEATURE_ID) AS feature_count
  FROM cells c
  LEFT JOIN SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.RELEASE_DT = '2025-06-06'
   AND f.H3_R8 = c.h3_cell
  GROUP BY c.cell_name, c.h3_cell
)
SELECT cell_name, h3_cell, source, feature_count
FROM counts
ORDER BY cell_name, source;

-- Expected 2026-08-28:
-- BX: FEMA 116, parcels 300, NYC 291
-- MN: FEMA 241, parcels 2343, NYC 2354
-- QN: FEMA 1108, parcels 1502, NYC 2007
