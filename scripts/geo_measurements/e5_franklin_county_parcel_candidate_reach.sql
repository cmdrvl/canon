-- E5 parcel-tier candidate reach: Franklin County, Ohio.
--
-- This is the first parcel-backed non-NYC measurement. It is deliberately
-- narrower than E5: Snowflake GEOGRAPHY is an empirical PIP oracle, not
-- Canon's quantized exact-local predicate; no deed or address field is truth;
-- and no precision, solver, or evidence-tier claim follows from reach alone.
--
-- Required live preparation: list and describe every referenced table before
-- execution. The build and source release below are immutable measurement
-- inputs. A later run is a new measurement, never an in-place substitution.
--
-- Fresh cmdrvl-data MCP result on 2026-09-01 (success envelope query_id null):
-- * 494,704 landed parcel rows / 494,043 geometrically admitted rows;
-- * 151 property subjects / 202 associated loans / 0 coordinate conflicts;
-- * 147 reached, 4 unreached; 146 unique PIP, 1 two-parcel PIP;
-- * 54,344 H3-blocked pairs, 148 PIP pairs, 10..1,242 blocked per subject;
-- * the four misses are 3.006..22.221 m from the nearest blocked parcel;
--   none is rescued by an invalid-retained source geometry.

WITH
params AS (
  SELECT
    'ce3953ac-c2d4-4b48-bf02-29f0cf341389'::TEXT AS bridge_build_id,
    '39049'::TEXT AS county_fips,
    'hub-de09f99cce0bcae7142d6d2e26582fd3-25'::TEXT AS parcel_release,
    '2026-09-01'::DATE AS parcel_release_dt,
    8::NUMBER(9,0) AS h3_resolution
),
raw_subjects AS (
  SELECT DISTINCT
    property_key,
    loan_key,
    latitude,
    longitude
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY
  WHERE build_id = (SELECT bridge_build_id FROM params)
    AND county_fips = (SELECT county_fips FROM params)
    AND property_key IS NOT NULL
    AND latitude IS NOT NULL
    AND longitude IS NOT NULL
),
subject_coordinate_stats AS (
  SELECT
    property_key,
    COUNT(DISTINCT TO_VARCHAR(latitude) || '|' || TO_VARCHAR(longitude))
      AS coordinate_pairs,
    MIN(latitude) AS latitude,
    MIN(longitude) AS longitude,
    COUNT(DISTINCT loan_key) AS loans
  FROM raw_subjects
  GROUP BY property_key
),
subjects AS (
  SELECT
    property_key,
    loans,
    ST_MAKEPOINT(longitude, latitude) AS point_geog,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(longitude, latitude),
      (SELECT h3_resolution FROM params)
    ) AS point_h3_r8
  FROM subject_coordinate_stats
  WHERE coordinate_pairs = 1
),
admissible_parcels AS (
  SELECT
    provider_feature_id,
    geom_geog
  FROM EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT
  WHERE release = (SELECT parcel_release FROM params)
    AND release_dt = (SELECT parcel_release_dt FROM params)
    AND county_fips = (SELECT county_fips FROM params)
    AND provider_feature_id IS NOT NULL
    AND geom_geog IS NOT NULL
    AND geom_parse_status = 'parsed'
    AND source_geometry_validity = 'valid'
    AND source_geom_wkb_sha256 IS NOT NULL
    AND geom_wgs84_sha256 IS NOT NULL
    AND h3_key_status = 'valid'
),
candidate_features AS (
  SELECT DISTINCT
    s.property_key,
    s.point_geog,
    c.provider_feature_id
  FROM subjects s
  JOIN EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_FEATURE_H3_COVERAGE c
    ON c.release = (SELECT parcel_release FROM params)
   AND c.release_dt = (SELECT parcel_release_dt FROM params)
   AND c.county_fips = (SELECT county_fips FROM params)
   AND c.h3_resolution = (SELECT h3_resolution FROM params)
   AND c.h3_cell = s.point_h3_r8
   AND c.h3_key_status = 'valid'
),
candidate_geometries AS (
  SELECT
    c.property_key,
    c.point_geog,
    c.provider_feature_id,
    p.geom_geog,
    p.geom_parse_status,
    p.source_geometry_validity,
    p.source_geom_wkb_sha256,
    p.geom_wgs84_sha256,
    p.h3_key_status
  FROM candidate_features c
  JOIN EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT p
    ON p.release = (SELECT parcel_release FROM params)
   AND p.release_dt = (SELECT parcel_release_dt FROM params)
   AND p.county_fips = (SELECT county_fips FROM params)
   AND p.provider_feature_id = c.provider_feature_id
),
pip_pairs AS (
  SELECT
    c.property_key,
    c.provider_feature_id
  FROM candidate_features c
  JOIN admissible_parcels p
    ON p.provider_feature_id = c.provider_feature_id
  WHERE ST_CONTAINS(p.geom_geog, c.point_geog)
),
per_subject AS (
  SELECT
    s.property_key,
    s.loans,
    COUNT(DISTINCT c.provider_feature_id) AS blocked_candidates,
    COUNT(DISTINCT p.provider_feature_id) AS containing_parcels
  FROM subjects s
  LEFT JOIN candidate_features c
    ON c.property_key = s.property_key
  LEFT JOIN pip_pairs p
    ON p.property_key = s.property_key
  GROUP BY s.property_key, s.loans
),
miss_diagnostics AS (
  SELECT
    c.property_key,
    COUNT_IF(
      c.geom_geog IS NOT NULL
      AND c.geom_parse_status = 'parsed'
      AND ST_CONTAINS(c.geom_geog, c.point_geog)
    ) AS any_parsed_contains,
    MIN(IFF(
      c.geom_geog IS NOT NULL,
      ST_DISTANCE(c.geom_geog, c.point_geog),
      NULL
    )) AS nearest_distance_m
  FROM candidate_geometries c
  JOIN per_subject s
    ON s.property_key = c.property_key
   AND s.containing_parcels = 0
  GROUP BY c.property_key
),
stats AS (
  SELECT
    (SELECT COUNT(DISTINCT property_key) FROM raw_subjects)
      AS raw_subject_properties,
    (SELECT COUNT(DISTINCT loan_key) FROM raw_subjects) AS raw_subject_loans,
    (SELECT COUNT(*) FROM subject_coordinate_stats WHERE coordinate_pairs <> 1)
      AS conflicting_coordinate_properties,
    (SELECT COUNT(*) FROM subjects) AS eligible_subject_properties,
    (SELECT COUNT(*) FROM admissible_parcels) AS admitted_parcels,
    COUNT_IF(containing_parcels > 0) AS reached_properties,
    COUNT_IF(containing_parcels = 0) AS unreached_properties,
    COUNT_IF(containing_parcels = 1) AS unique_pip_properties,
    COUNT_IF(containing_parcels > 1) AS multi_pip_properties,
    MIN(blocked_candidates) AS min_blocked_candidates,
    MAX(blocked_candidates) AS max_blocked_candidates,
    SUM(blocked_candidates) AS blocked_candidate_pairs,
    MIN(containing_parcels) AS min_containing_parcels,
    MAX(containing_parcels) AS max_containing_parcels,
    SUM(containing_parcels) AS pip_pairs
  FROM per_subject
),
miss_stats AS (
  SELECT
    COUNT(*) AS diagnosed_misses,
    COUNT_IF(any_parsed_contains > 0) AS invalid_geometry_rescues,
    MIN(nearest_distance_m) AS min_nearest_distance_m,
    MAX(nearest_distance_m) AS max_nearest_distance_m,
    COUNT_IF(nearest_distance_m <= 10) AS within_10m,
    COUNT_IF(nearest_distance_m <= 100) AS within_100m,
    COUNT_IF(nearest_distance_m > 500) AS over_500m
  FROM miss_diagnostics
)
SELECT OBJECT_CONSTRUCT_KEEP_NULL(
  'row_contract', 'canon_geo_e5_franklin_parcel_candidate_reach.v0',
  'proof_scope', 'snowflake_geography_oracle_over_h3_r8_feature_coverage',
  'bridge_build_id', params.bridge_build_id,
  'parcel_release', params.parcel_release,
  'parcel_release_dt', TO_VARCHAR(params.parcel_release_dt, 'YYYY-MM-DD'),
  'county_fips', params.county_fips,
  'h3_resolution', params.h3_resolution,
  'raw_subject_properties', stats.raw_subject_properties,
  'raw_subject_loans', stats.raw_subject_loans,
  'conflicting_coordinate_properties', stats.conflicting_coordinate_properties,
  'eligible_subject_properties', stats.eligible_subject_properties,
  'admitted_parcels', stats.admitted_parcels,
  'reached_properties', stats.reached_properties,
  'unreached_properties', stats.unreached_properties,
  'unique_pip_properties', stats.unique_pip_properties,
  'multi_pip_properties', stats.multi_pip_properties,
  'min_blocked_candidates', stats.min_blocked_candidates,
  'max_blocked_candidates', stats.max_blocked_candidates,
  'blocked_candidate_pairs', stats.blocked_candidate_pairs,
  'min_containing_parcels', stats.min_containing_parcels,
  'max_containing_parcels', stats.max_containing_parcels,
  'pip_pairs', stats.pip_pairs,
  'miss_diagnostic', OBJECT_CONSTRUCT_KEEP_NULL(
    'diagnosed_misses', miss_stats.diagnosed_misses,
    'invalid_geometry_rescues', miss_stats.invalid_geometry_rescues,
    'min_nearest_distance_m', miss_stats.min_nearest_distance_m,
    'max_nearest_distance_m', miss_stats.max_nearest_distance_m,
    'within_10m', miss_stats.within_10m,
    'within_100m', miss_stats.within_100m,
    'over_500m', miss_stats.over_500m
  ),
  'guard_status', IFF(
    stats.raw_subject_properties > 0
    AND stats.conflicting_coordinate_properties = 0
    AND stats.eligible_subject_properties = stats.raw_subject_properties
    AND stats.admitted_parcels > 0
    AND stats.reached_properties + stats.unreached_properties
      = stats.eligible_subject_properties
    AND stats.unique_pip_properties + stats.multi_pip_properties
      = stats.reached_properties
    AND miss_stats.diagnosed_misses = stats.unreached_properties,
    'ok',
    'refuse'
  )
) AS result
FROM params
CROSS JOIN stats
CROSS JOIN miss_stats;
