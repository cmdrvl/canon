-- Output grain: one pinned source observation inside the 150 m disc.
-- The component calculation and expected block are documented in README.md.
WITH anchor AS (
  SELECT ST_POINT(-73.9580550, 40.7688430) AS geog
), observations AS (
  SELECT 'parcel' AS source,
         COALESCE(NULLIF(BBL, ''), 'row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS native_id,
         CENTROID_LON AS lon,
         CENTROID_LAT AS lat
  FROM SOURCE.NYC_DCP_MAPPLUTO_HOT, anchor
  WHERE RELEASE = '26v1'
    AND RELEASE_DT = '2026-05-01'
    AND CENTROID_GEOG IS NOT NULL
    AND ST_DWITHIN(CENTROID_GEOG, anchor.geog, 150)
  UNION ALL
  SELECT 'footprint' AS source,
         COALESCE(TO_VARCHAR(OBJECTID), 'row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS native_id,
         CENTROID_LON AS lon,
         CENTROID_LAT AS lat
  FROM SOURCE.NYC_BUILDING_FOOTPRINTS_HOT, anchor
  WHERE RELEASE_DT = '2026-08-09'
    AND IS_ACTIVE_FOOTPRINT = TRUE
    AND CENTROID_GEOG IS NOT NULL
    AND ST_DWITHIN(CENTROID_GEOG, anchor.geog, 150)
)
SELECT source, native_id, lon, lat
FROM observations
ORDER BY source, native_id;

-- Expected denominator: 100 parcels + 93 footprints = 193 observations.
