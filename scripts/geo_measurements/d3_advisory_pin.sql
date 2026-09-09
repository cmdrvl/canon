-- D3/G3 archived advisory extraction probe for bd-2ocv.
--
-- Grain: one row per wind-radii threshold for the selected real NHC/CPHC
-- advisory. This query deliberately selects initial_wind_radii, not the
-- forecast cone and not forecast_wind_radii. The current GeoAdvisoryPin
-- contract carries one polygon per knots band; cp012026 advisory 033A has a
-- single initial radius polygon for each of 34/50/64 knots, while its forecast
-- wind-radii product has multiple rows per band and needs a future contract if
-- used.
--
-- Required live preparation before execution:
-- * describe SOURCE.GEO_NHC_TROPICAL_NORMALIZED_GEOMETRY.
-- * Keep SOURCE_SHA256 visible. The landed view currently exposes SHA256 of
--   the downloaded ZIP; a source-object BLAKE3 pin must be supplied by the
--   retained artifact/archive layer before this can become a GeoAdvisoryPin
--   cited as G3 proof.

WITH
selected AS (
  SELECT
    storm_id,
    storm_name,
    basin,
    advisory,
    issued_at,
    product,
    wind_threshold_kt,
    source_artifact,
    source_member,
    source_record_index,
    geometry_type,
    geometry_geojson,
    source_url,
    source_sha256,
    license_terms,
    attribution_text,
    parser_version,
    filename
  FROM EDGAR_DB.SOURCE.GEO_NHC_TROPICAL_NORMALIZED_GEOMETRY
  WHERE storm_id = 'cp012026'
    AND advisory = '033A'
    AND product = 'initial_wind_radii'
    AND wind_threshold_kt IN (34, 50, 64)
),
forecast_shape_check AS (
  SELECT
    wind_threshold_kt,
    COUNT(*) AS forecast_rows
  FROM EDGAR_DB.SOURCE.GEO_NHC_TROPICAL_NORMALIZED_GEOMETRY
  WHERE storm_id = 'cp012026'
    AND advisory = '033A'
    AND product = 'forecast_wind_radii'
    AND wind_threshold_kt IN (34, 50, 64)
  GROUP BY wind_threshold_kt
)
SELECT
  'canon_geo_d3_advisory_pin_source_rows.v0' AS row_contract,
  storm_id,
  storm_name,
  basin,
  advisory,
  TO_VARCHAR(issued_at, 'YYYY-MM-DD HH24:MI:SS') AS issued_at_utc,
  product,
  wind_threshold_kt,
  source_artifact,
  source_member,
  source_record_index,
  geometry_type,
  ARRAY_SIZE(geometry_geojson:coordinates[0]) AS exterior_vertex_count,
  source_url,
  source_sha256,
  license_terms,
  attribution_text,
  parser_version,
  filename,
  (
    SELECT OBJECT_AGG(wind_threshold_kt::TEXT, forecast_rows)
    FROM forecast_shape_check
  ) AS forecast_wind_radii_rows_by_threshold,
  'initial_wind_radii_fit_current_one_polygon_per_band_contract' AS pin_contract_fit,
  'forecast_cone_is_not_impact_footprint' AS cone_exclusion_reason
FROM selected
ORDER BY wind_threshold_kt, source_record_index;
