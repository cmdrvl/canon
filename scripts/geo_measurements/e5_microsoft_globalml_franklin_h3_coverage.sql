-- E5 source-availability denominator: Microsoft GlobalML footprints around
-- the current Franklin County, Ohio collateral bridge.
--
-- This is a bounded source-availability measurement, not an E5 evaluation, not
-- parcel reach, not precision, and not a four-independent-votes claim. The
-- global footprint source is filtered to the same Franklin subject work cells
-- used by the parcel-backed reach successor so the source can enter E5 as an
-- ordinary building-footprint evidence class without jurisdiction-specific
-- core dispatch.
--
-- Required live preparation: describe LOAN_ISSUANCE_PROPERTY,
-- MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_H3_COVERAGE_HOT, and
-- MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT before execution. The coverage
-- query honors the coverage bridge cluster key with state and h3_cell filters.
--
-- Fresh cmdrvl-data MCP result on 2026-09-08 (measurement
-- bd-1wmw_microsoft_globalml_current_franklin_h3_coverage):
-- * 151 property subjects / 202 associated loans / 114 center cells;
-- * 585 center+k1 r8 work cells;
-- * 168,778 coverage rows / distinct features in 581 occupied work cells;
-- * every coverage feature joins to a HOT geometry row for the same release.

WITH
params AS (
  SELECT
    '80d0ea39-a5aa-4c27-a8d7-f662a4507257'::TEXT AS bridge_build_id,
    '39049'::TEXT AS county_fips,
    'OH'::TEXT AS state,
    '2026-07-24'::DATE AS release_dt,
    8::NUMBER(9,0) AS h3_resolution,
    1::NUMBER(9,0) AS halo_k
),
subjects AS (
  SELECT DISTINCT
    property_key,
    loan_key,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(longitude, latitude),
      (SELECT h3_resolution FROM params)
    ) AS center_cell
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY
  WHERE build_id = (SELECT bridge_build_id FROM params)
    AND county_fips = (SELECT county_fips FROM params)
    AND property_key IS NOT NULL
    AND latitude IS NOT NULL
    AND longitude IS NOT NULL
),
work_cells AS (
  SELECT DISTINCT
    H3_STRING_TO_INT(cell.value::TEXT) AS h3_cell
  FROM (
    SELECT DISTINCT center_cell
    FROM subjects
  ) centers,
  LATERAL FLATTEN(
    input => H3_GRID_DISK(
      centers.center_cell,
      (SELECT halo_k FROM params)
    )
  ) cell
),
coverage_rows AS (
  SELECT
    c.provider_feature_id,
    c.h3_cell,
    c.coverage_method
  FROM EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_H3_COVERAGE_HOT c
  JOIN work_cells w
    ON w.h3_cell = c.h3_cell
  WHERE c.state = (SELECT state FROM params)
    AND c.release_dt = (SELECT release_dt FROM params)
    AND c.h3_resolution = (SELECT h3_resolution FROM params)
),
hot_rows AS (
  SELECT DISTINCT
    h.provider_feature_id
  FROM EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT h
  WHERE h.state = (SELECT state FROM params)
    AND h.release_dt = (SELECT release_dt FROM params)
    AND h.geom_geog IS NOT NULL
    AND h.h3_r8 IS NOT NULL
)
SELECT OBJECT_CONSTRUCT_KEEP_NULL(
  'row_contract',
    'canon_geo_e5_microsoft_globalml_franklin_h3_coverage.v0',
  'proof_scope', 'h3_r8_center_plus_k1_coverage_denominator',
  'bridge_build_id', params.bridge_build_id,
  'county_fips', params.county_fips,
  'state', params.state,
  'release_dt', TO_VARCHAR(params.release_dt, 'YYYY-MM-DD'),
  'h3_resolution', params.h3_resolution,
  'halo_k', params.halo_k,
  'subject_properties',
    (SELECT COUNT(DISTINCT property_key) FROM subjects),
  'subject_loans',
    (SELECT COUNT(DISTINCT loan_key) FROM subjects),
  'subject_center_cells',
    (SELECT COUNT(DISTINCT center_cell) FROM subjects),
  'work_cells',
    (SELECT COUNT(*) FROM work_cells),
  'coverage_rows',
    COUNT(*),
  'distinct_coverage_features',
    COUNT(DISTINCT coverage_rows.provider_feature_id),
  'occupied_work_cells',
    COUNT(DISTINCT coverage_rows.h3_cell),
  'features_with_hot_geometry',
    COUNT(DISTINCT IFF(
      hot_rows.provider_feature_id IS NOT NULL,
      coverage_rows.provider_feature_id,
      NULL
    )),
  'features_without_hot_geometry',
    COUNT(DISTINCT IFF(
      hot_rows.provider_feature_id IS NULL,
      coverage_rows.provider_feature_id,
      NULL
    )),
  'coverage_methods',
    COUNT(DISTINCT coverage_rows.coverage_method),
  'guard_status', IFF(
    (SELECT COUNT(DISTINCT property_key) FROM subjects) > 0
    AND (SELECT COUNT(*) FROM work_cells) > 0
    AND COUNT(DISTINCT coverage_rows.provider_feature_id) > 0
    AND COUNT(DISTINCT IFF(
      hot_rows.provider_feature_id IS NULL,
      coverage_rows.provider_feature_id,
      NULL
    )) = 0,
    'ok',
    'refuse'
  )
) AS result
FROM params
CROSS JOIN coverage_rows
LEFT JOIN hot_rows
  ON hot_rows.provider_feature_id = coverage_rows.provider_feature_id
GROUP BY
  params.bridge_build_id,
  params.county_fips,
  params.state,
  params.release_dt,
  params.h3_resolution,
  params.halo_k;
