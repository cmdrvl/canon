-- E5 live source-byte probe: one runtime-selected Franklin parcel candidate.
--
-- This query emits a selection receipt and one complete
-- canon_geo_warehouse_geometry_rows.v0 request. The selection is seeded over
-- the live admitted PIP population; no property or parcel id is hard-coded.
-- The 10,000-foot grid coordinates are part of frame_id, so the explicit
-- source-plane origin cannot silently drift when other evidence accretes.
--
-- This is a positive transport/geometry-admission probe, not population-scale
-- proof, candidate accuracy, or E5 closure. Snowflake validates the decoded
-- base64 digest before Canon independently validates it again.
--
-- Fresh file-exact 2026-09-01 result (success envelope query_id null): 148
-- eligible PIP rows, one 29-vertex seeded selection, and decoded digest match.
-- The emitted GEOMETRY_REQUEST was consumed by the public Canon command; the
-- resulting fixed-point receipt is recorded in the measurement README.

WITH
params AS (
  SELECT
    'ce3953ac-c2d4-4b48-bf02-29f0cf341389'::TEXT AS bridge_build_id,
    '39049'::TEXT AS county_fips,
    'hub-de09f99cce0bcae7142d6d2e26582fd3-25'::TEXT AS parcel_release,
    '2026-09-01'::DATE AS parcel_release_dt,
    'canon-e5-franklin-live-geometry-2026-09-01-v0'::TEXT AS selection_seed,
    8::NUMBER(9,0) AS blocking_h3_resolution,
    9::NUMBER(9,0) AS ownership_h3_resolution,
    9::NUMBER(9,0) AS source_decimal_places,
    10000::NUMBER(38,0) AS source_grid_feet,
    10000000::NUMBER(38,0) AS max_abs_coordinate_mm,
    10000::NUMBER(38,0) AS max_vertices_per_geometry,
    10000000::NUMBER(38,0) AS max_geometry_bytes_per_tile
),
subjects AS (
  SELECT
    property_key,
    ST_MAKEPOINT(MIN(longitude), MIN(latitude)) AS point_geog,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(MIN(longitude), MIN(latitude)),
      (SELECT blocking_h3_resolution FROM params)
    ) AS point_h3_r8,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(MIN(longitude), MIN(latitude)),
      (SELECT ownership_h3_resolution FROM params)
    ) AS point_h3_r9
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY
  WHERE build_id = (SELECT bridge_build_id FROM params)
    AND county_fips = (SELECT county_fips FROM params)
    AND property_key IS NOT NULL
    AND latitude IS NOT NULL
    AND longitude IS NOT NULL
  GROUP BY property_key
  HAVING COUNT(DISTINCT TO_VARCHAR(latitude) || '|' || TO_VARCHAR(longitude)) = 1
),
pip AS (
  SELECT DISTINCT
    s.property_key,
    s.point_h3_r9,
    c.provider_feature_id
  FROM subjects s
  JOIN EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_FEATURE_H3_COVERAGE c
    ON c.release = (SELECT parcel_release FROM params)
   AND c.release_dt = (SELECT parcel_release_dt FROM params)
   AND c.county_fips = (SELECT county_fips FROM params)
   AND c.h3_resolution = (SELECT blocking_h3_resolution FROM params)
   AND c.h3_cell = s.point_h3_r8
   AND c.h3_key_status = 'valid'
  JOIN EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT p
    ON p.release = c.release
   AND p.release_dt = c.release_dt
   AND p.county_fips = c.county_fips
   AND p.provider_feature_id = c.provider_feature_id
   AND p.geom_geog IS NOT NULL
   AND p.geom_parse_status = 'parsed'
   AND p.source_geometry_validity = 'valid'
  WHERE ST_CONTAINS(p.geom_geog, s.point_geog)
),
eligible AS (
  SELECT
    p.property_key,
    p.point_h3_r9,
    p.provider_feature_id,
    e.source_archive_sha256,
    e.source_geom_wkb,
    e.source_geom_wkb_sha256,
    e.source_geom_srid,
    e.source_crs_identifier,
    e.geometry_evidence_contract_version,
    e.transform_execution_id,
    e.transform_definition_id,
    e.source_vertex_count,
    FLOOR(e.source_bbox_xmin / (SELECT source_grid_feet FROM params))
      * (SELECT source_grid_feet FROM params) AS source_origin_x,
    FLOOR(e.source_bbox_ymin / (SELECT source_grid_feet FROM params))
      * (SELECT source_grid_feet FROM params) AS source_origin_y,
    SHA2_HEX(
      (SELECT selection_seed FROM params)
      || '|' || p.property_key || '|' || p.provider_feature_id,
      256
    ) AS selection_rank,
    COUNT(*) OVER () AS eligible_rows
  FROM pip p
  JOIN EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_GEOMETRY_EVIDENCE_EXT e
    ON e.release = (SELECT parcel_release FROM params)
   AND e.release_dt = (SELECT parcel_release_dt FROM params)
   AND e.county = (SELECT county_fips FROM params)
   AND e.provider_feature_id = p.provider_feature_id
   AND e.source_geometry_validity = 'valid'
   AND e.source_geom_wkb IS NOT NULL
   AND e.source_geom_wkb_sha256 IS NOT NULL
   AND e.source_vertex_count > 0
   AND e.source_vertex_count <= (SELECT max_vertices_per_geometry FROM params)
),
chosen AS (
  SELECT *
  FROM eligible
  QUALIFY ROW_NUMBER() OVER (
    ORDER BY selection_rank, property_key, provider_feature_id
  ) = 1
)
SELECT
  OBJECT_CONSTRUCT_KEEP_NULL(
    'row_contract', 'canon_geo_live_warehouse_geometry_probe.v0',
    'proof_scope', 'one_seeded_runtime_selected_franklin_pip_candidate',
    'selection_seed', params.selection_seed,
    'eligible_rows', chosen.eligible_rows,
    'bridge_build_id', params.bridge_build_id,
    'parcel_release', params.parcel_release,
    'parcel_release_dt', TO_VARCHAR(params.parcel_release_dt, 'YYYY-MM-DD'),
    'property_key', chosen.property_key,
    'provider_feature_id', chosen.provider_feature_id,
    'source_vertex_count', chosen.source_vertex_count,
    'decoded_digest_matches', IFF(
      SHA2_HEX(BASE64_DECODE_BINARY(chosen.source_geom_wkb), 256)
        = chosen.source_geom_wkb_sha256,
      TRUE,
      FALSE
    )
  ) AS selection_receipt,
  OBJECT_CONSTRUCT_KEEP_NULL(
    'version', 'canon_geo_warehouse_geometry_rows.v0',
    'tile_id', chosen.point_h3_r9,
    'frame_id',
      'h3r9:' || chosen.point_h3_r9
      || ':epsg3735-grid10000ft:x' || TO_VARCHAR(chosen.source_origin_x)
      || ':y' || TO_VARCHAR(chosen.source_origin_y) || ':mm:v0',
    'source_crs', chosen.source_crs_identifier,
    'source_srid', chosen.source_geom_srid,
    'source_decimal_places', params.source_decimal_places,
    'source_origin', OBJECT_CONSTRUCT(
      'x', TO_VARCHAR(chosen.source_origin_x),
      'y', TO_VARCHAR(chosen.source_origin_y)
    ),
    'source_unit_to_millimetres', OBJECT_CONSTRUCT(
      'unit_id', 'us-survey-foot',
      'numerator', 1200000,
      'denominator', 3937
    ),
    'rows', ARRAY_CONSTRUCT(OBJECT_CONSTRUCT_KEEP_NULL(
      'feature_id', chosen.provider_feature_id,
      'source_record_id',
        'EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_GEOMETRY_EVIDENCE_EXT:'
        || params.parcel_release || ':' || chosen.provider_feature_id,
      'source_dataset', 'franklin_county_auditor_tax_parcels',
      'source_release', params.parcel_release,
      'source_release_date', TO_VARCHAR(params.parcel_release_dt, 'YYYY-MM-DD'),
      'source_geometry_contract_version',
        chosen.geometry_evidence_contract_version,
      'source_archive_sha256', chosen.source_archive_sha256,
      'source_crs', chosen.source_crs_identifier,
      'source_srid', chosen.source_geom_srid,
      'source_geom_wkb_base64', chosen.source_geom_wkb,
      'source_geom_wkb_sha256', chosen.source_geom_wkb_sha256,
      'transform_execution_id', chosen.transform_execution_id,
      'transform_definition_id', chosen.transform_definition_id
    )),
    'max_abs_coordinate_mm', params.max_abs_coordinate_mm,
    'max_vertices_per_geometry', params.max_vertices_per_geometry,
    'max_geometry_bytes_per_tile', params.max_geometry_bytes_per_tile
  ) AS geometry_request
FROM params
CROSS JOIN chosen;
