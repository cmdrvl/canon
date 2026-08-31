-- Appendix H.7 bounded r8+k1 raw-observation incidence measurement.
--
-- This query consumes one successful h7_staging_accepted_truth_row.v0 result,
-- assigns its collateral points to deterministic r8 home-cell shards, and
-- measures majority-overlap incidence inside each selected center+k1 section.
-- The output is one row per center cell, never one monolithic loan candidate
-- union. Exact solving remains section -> incidence components -> small exact
-- residuals.
--
-- The components here contain MapPLUTO parcel nodes and raw NYC/Overture
-- observation nodes. They are predicate-incidence measurements, not final
-- solver widths: cross-source observations can share OSM lineage or represent
-- the same latent building, while future evidence can couple current stars.
-- Source count is provenance, not independent information.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'
--   __BD2B9D_H7_SHARD_COUNT__
--   __BD2B9D_H7_SHARD_INDEX__

WITH
params AS (
  SELECT
    'h7_staging_accepted_truth_row.v0'::TEXT
      AS expected_accepted_truth_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    8::NUMBER(9,0) AS h3_resolution,
    1::NUMBER(9,0) AS halo_k,
    __BD2B9D_H7_SHARD_COUNT__::NUMBER(9,0) AS shard_count,
    __BD2B9D_H7_SHARD_INDEX__::NUMBER(9,0) AS shard_index,
    200::NUMBER(38,0) AS accepted_truth_row_cap
),
filed_county_map AS (
  SELECT * FROM VALUES
    ('NEW YORK', 1), ('MANHATTAN', 1), ('NY061', 1),
    ('BRONX', 2),
    ('KINGS', 3), ('BROOKLYN', 3),
    ('QUEENS', 4),
    ('RICHMOND', 5)
  AS m(propertycounty, filed_borough)
),
accepted AS (
  SELECT *
  FROM TABLE(RESULT_SCAN('__BD7BCP_H7_ACCEPTED_TRUTH_QUERY_ID__'))
),
accepted_stats AS (
  SELECT
    COUNT(*) AS accepted_rows,
    COUNT(DISTINCT loan_key) AS accepted_loans,
    COUNT_IF(row_contract <>
      (SELECT expected_accepted_truth_contract FROM params))
      AS contract_mismatch_rows,
    COUNT_IF(bridge_build_id <> (SELECT bridge_build_id FROM params))
      AS bridge_build_mismatch_rows,
    COUNT_IF(NOT export_row_cap_reconciles) AS upstream_row_cap_failures,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS truth_bbl_count_mismatch_rows
  FROM accepted
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT 'accepted_truth_result_empty' AS failure_reason,
      (SELECT accepted_rows FROM accepted_stats) = 0 AS failed
    UNION ALL
    SELECT 'accepted_truth_result_exceeds_bound',
      (SELECT accepted_rows FROM accepted_stats)
        > (SELECT accepted_truth_row_cap FROM params)
    UNION ALL
    SELECT 'accepted_truth_repeats_loan',
      (SELECT accepted_rows FROM accepted_stats)
        <> (SELECT accepted_loans FROM accepted_stats)
    UNION ALL
    SELECT 'accepted_truth_contract_mismatch',
      (SELECT contract_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_bridge_build_mismatch',
      (SELECT bridge_build_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_upstream_row_cap_failure',
      (SELECT upstream_row_cap_failures FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'accepted_truth_bbl_count_mismatch',
      (SELECT truth_bbl_count_mismatch_rows FROM accepted_stats) <> 0
    UNION ALL
    SELECT 'invalid_shard_count',
      (SELECT shard_count FROM params) <= 0
    UNION ALL
    SELECT 'invalid_shard_index',
      (SELECT shard_index FROM params) < 0
      OR (SELECT shard_index FROM params) >= (SELECT shard_count FROM params)
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    MIN(failure_reason) AS refusal_reason
  FROM guard_failures
),
points AS (
  SELECT DISTINCT
    a.loan_key,
    lip.property_key,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(lip.longitude, lip.latitude),
      (SELECT h3_resolution FROM params)
    ) AS center_cell
  FROM accepted a
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = (SELECT bridge_build_id FROM params)
   AND lip.loan_key = a.loan_key
  JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
  WHERE (SELECT guard_status FROM guard_summary) = 'ok'
    AND lip.propertystate = 'NY'
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
    AND ARRAY_CONTAINS(m.filed_borough::VARIANT, a.filed_boroughs)
),
center_cells AS (
  SELECT
    center_cell,
    COUNT(DISTINCT loan_key) AS linked_loans,
    COUNT(DISTINCT property_key) AS linked_properties
  FROM points
  WHERE MOD(
    ABS(MD5_NUMBER_LOWER64(center_cell)),
    (SELECT shard_count FROM params)
  ) = (SELECT shard_index FROM params)
  GROUP BY center_cell
),
work_cells AS (
  SELECT
    c.center_cell,
    H3_STRING_TO_INT(cell.value::TEXT) AS work_cell_int
  FROM center_cells c,
    LATERAL FLATTEN(
      input => H3_GRID_DISK(
        c.center_cell,
        (SELECT halo_k FROM params)
      )
    ) cell
),
work_parcel_keys AS (
  SELECT DISTINCT
    w.center_cell,
    h.bbl_key AS parcel_id
  FROM work_cells w
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_GEOMETRY_HOT_KEYS h
    ON h.h3_r8_int = w.work_cell_int
  WHERE h.source_system = 'nyc_dcp'
    AND h.source_table = 'nyc_dcp_mappluto_hot'
    AND h.dataset = 'mappluto'
    AND h.state_key = 'NY'
    AND h.release = '26v2'
    AND h.release_dt = '2026-08-01'
    AND h.feature_type = 'shoreline_clipped'
    AND h.bbl_key_status = 'valid'
    AND h.h3_r8_status = 'valid'
    AND h.key_validation_status = 'valid'
),
work_parcels AS (
  SELECT
    k.center_cell,
    k.parcel_id,
    TO_GEOGRAPHY(p.geom_wkt) AS geom,
    p.bbox_xmin,
    p.bbox_ymin,
    p.bbox_xmax,
    p.bbox_ymax
  FROM work_parcel_keys k
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT p
    ON p.release = '26v2'
   AND p.release_dt = '2026-08-01'
   AND p."VARIANT" = 'shoreline_clipped'
   -- The raw Parquet NUMBER renders as e.g. 1000010010.0 while the staging
   -- key is the ten-digit BBL. Omitting this normalization silently empties
   -- the local parcel plane even though metadata counts remain nonzero.
   AND REGEXP_REPLACE(TO_CHAR(p.bbl), '[.]0$', '') = k.parcel_id
  WHERE p.geom_wkt IS NOT NULL
),
global_parcels AS (
  SELECT
    REGEXP_REPLACE(TO_CHAR(p.bbl), '[.]0$', '') AS parcel_id,
    TO_GEOGRAPHY(p.geom_wkt) AS geom,
    p.bbox_xmin,
    p.bbox_ymin,
    p.bbox_xmax,
    p.bbox_ymax
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT p
  WHERE p.release = '26v2'
    AND p.release_dt = '2026-08-01'
    AND p."VARIANT" = 'shoreline_clipped'
    AND p.bbl IS NOT NULL
    AND p.geom_wkt IS NOT NULL
),
target_observations AS (
  SELECT
    c.center_cell,
    'nyc_footprint'::TEXT AS source_name,
    f.objectid::TEXT AS observation_id,
    f.geom_geog AS geom,
    f.bbox_xmin,
    f.bbox_ymin,
    f.bbox_xmax,
    f.bbox_ymax,
    NULLIF(ST_AREA(f.geom_geog), 0) AS computed_area_m2
  FROM center_cells c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f
    ON f.h3_r8 = c.center_cell
  WHERE f.release_dt = '2026-08-09'
    AND f.is_active_footprint = TRUE
    AND f.objectid IS NOT NULL
    AND f.geom_geog IS NOT NULL

  UNION ALL

  SELECT
    c.center_cell,
    'overture_building'::TEXT AS source_name,
    o.provider_feature_id::TEXT AS observation_id,
    o.geom_geog AS geom,
    o.bbox_xmin,
    o.bbox_ymin,
    o.bbox_xmax,
    o.bbox_ymax,
    NULLIF(ST_AREA(o.geom_geog), 0) AS computed_area_m2
  FROM center_cells c
  JOIN EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
    ON o.h3_r8 = c.center_cell
  WHERE o.release = '2026-07-22.0'
    AND o.release_dt = '2026-07-22'
    AND o.country = 'US'
    AND o.state = 'US-NY'
    AND o.dataset = 'buildings'
    AND o.feature_type = 'building'
    AND o.license_class = 'odbl'
    AND o.h3_key_status = 'valid'
    AND o.provider_feature_id IS NOT NULL
    AND o.geom_geog IS NOT NULL
),
majority_pairs AS (
  SELECT
    o.center_cell,
    o.source_name,
    o.observation_id,
    p.parcel_id,
    w.parcel_id IS NOT NULL AS in_k1
  FROM target_observations o
  JOIN global_parcels p
    ON p.bbox_xmax >= o.bbox_xmin
   AND p.bbox_xmin <= o.bbox_xmax
   AND p.bbox_ymax >= o.bbox_ymin
   AND p.bbox_ymin <= o.bbox_ymax
   AND ST_INTERSECTS(o.geom, p.geom)
  LEFT JOIN work_parcel_keys w
    ON w.center_cell = o.center_cell
   AND w.parcel_id = p.parcel_id
  WHERE o.computed_area_m2 IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(o.geom, p.geom)) / o.computed_area_m2 > 0.5
),
per_observation AS (
  SELECT
    o.center_cell,
    o.source_name,
    o.observation_id,
    COUNT(mp.parcel_id) AS global_majority_count,
    COUNT_IF(mp.in_k1) AS k1_majority_count
  FROM target_observations o
  LEFT JOIN majority_pairs mp
    ON mp.center_cell = o.center_cell
   AND mp.source_name = o.source_name
   AND mp.observation_id = o.observation_id
  GROUP BY o.center_cell, o.source_name, o.observation_id
),
-- With no multi-majority observations, each connected component is a parcel
-- star plus its incident observations, or one isolated observation. The final
-- component_shape_sanity refuses to interpret this shortcut otherwise.
parcel_components AS (
  SELECT
    p.center_cell,
    1 + COUNT(DISTINCT mp.source_name || ':' || mp.observation_id)
      AS component_size
  FROM work_parcels p
  LEFT JOIN majority_pairs mp
    ON mp.center_cell = p.center_cell
   AND mp.parcel_id = p.parcel_id
   AND mp.in_k1
  GROUP BY p.center_cell, p.parcel_id
),
zero_observation_components AS (
  SELECT center_cell, 1 AS component_size
  FROM per_observation
  WHERE k1_majority_count = 0
),
components AS (
  SELECT * FROM parcel_components
  UNION ALL
  SELECT * FROM zero_observation_components
),
component_stats AS (
  SELECT
    center_cell,
    COUNT(*) AS component_count,
    SUM(component_size) AS component_nodes,
    MEDIAN(component_size) AS median_component_size,
    APPROX_PERCENTILE(component_size, 0.9) AS p90_component_size,
    MAX(component_size) AS max_component_size
  FROM components
  GROUP BY center_cell
),
parcel_counts AS (
  SELECT center_cell, COUNT(*) AS work_parcels
  FROM work_parcels
  GROUP BY center_cell
),
observation_counts AS (
  SELECT
    center_cell,
    COUNT(*) AS target_observations,
    COUNT_IF(source_name = 'nyc_footprint') AS nyc_footprints,
    COUNT_IF(source_name = 'overture_building') AS overture_buildings,
    COUNT_IF(global_majority_count = 1 AND k1_majority_count = 0)
      AS truth_outside_k1,
    COUNT_IF(k1_majority_count > 1) AS k1_multi
  FROM per_observation
  GROUP BY center_cell
)
SELECT
  (SELECT guard_status FROM guard_summary) AS guard_status,
  (SELECT refusal_reason FROM guard_summary) AS refusal_reason,
  (SELECT shard_count FROM params) AS shard_count,
  (SELECT shard_index FROM params) AS shard_index,
  c.center_cell,
  c.linked_loans,
  c.linked_properties,
  COALESCE(p.work_parcels, 0) AS work_parcels,
  COALESCE(o.target_observations, 0) AS target_observations,
  COALESCE(o.nyc_footprints, 0) AS nyc_footprints,
  COALESCE(o.overture_buildings, 0) AS overture_buildings,
  COALESCE(o.truth_outside_k1, 0) AS truth_outside_k1,
  COALESCE(o.k1_multi, 0) AS k1_multi,
  COALESCE(s.component_count, 0) AS component_count,
  COALESCE(s.component_nodes, 0) AS component_nodes,
  COALESCE(s.median_component_size, 0) AS median_component_size,
  COALESCE(s.p90_component_size, 0) AS p90_component_size,
  COALESCE(s.max_component_size, 0) AS max_component_size,
  IFF(
    COALESCE(p.work_parcels, 0) > 0
      AND COALESCE(o.target_observations, 0) > 0,
    'PASS',
    'FAIL'
  ) AS nonzero_work_unit_sanity,
  IFF(COALESCE(o.k1_multi, 0) = 0, 'PASS', 'REFUSED')
    AS component_shape_sanity,
  IFF(
    COALESCE(s.component_nodes, 0)
      = COALESCE(p.work_parcels, 0) + COALESCE(o.target_observations, 0),
    'PASS',
    'FAIL'
  ) AS component_accounting_sanity
FROM center_cells c
LEFT JOIN parcel_counts p USING (center_cell)
LEFT JOIN observation_counts o USING (center_cell)
LEFT JOIN component_stats s USING (center_cell)
ORDER BY c.center_cell
LIMIT 200
