-- E5 preflight: generic thin-tier availability near Franklin County, Ohio
-- collateral points.
--
-- This is a bounded source-availability measurement, not an E5 evaluation and
-- not a geometric candidate-reach claim. H3 supplies only the r8 center+k1
-- blocking section. No address field, NYC-specific source, truth label, parcel
-- predicate, or solver result participates.
--
-- The county parcel layer required to repeat E1-E4 is not present in the
-- 2026-08-31 warehouse inventory. This query proves only that four generic
-- evidence classes are nonempty near pinned collateral subjects while that
-- landing remains outstanding.

WITH
params AS (
  SELECT
    '39049'::TEXT AS county_fips,
    'Franklin County, Ohio'::TEXT AS geography,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    8::NUMBER(9,0) AS h3_resolution,
    1::NUMBER(9,0) AS halo_k,
    'r8_center_plus_k1_source_availability'::TEXT AS measurement_scope
),
required_sources AS (
  SELECT *
  FROM VALUES
    ('fema_structures', 'fema', 'fema_usa_structures_hot',
      NULL, '2023-05-02', 'usa_structures'),
    ('microsoft_footprints', 'microsoft',
      'microsoft_globalml_building_footprints_hot', NULL, '2026-07-24',
      'globalml_building_footprints'),
    ('overture_addresses', 'overture_maps', 'overture_maps_features_hot',
      '2026-07-22.0', '2026-07-22', 'addresses'),
    ('overture_buildings', 'overture_maps', 'overture_maps_features_hot',
      '2026-07-22.0', '2026-07-22', 'buildings')
    AS s(
      evidence_class,
      source_system,
      source_table,
      release,
      release_dt,
      dataset
    )
),
subject_rows AS (
  SELECT DISTINCT
    lip.property_key,
    lip.loan_key,
    lip.loan_property_count,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(lip.longitude, lip.latitude),
      (SELECT h3_resolution FROM params)
    ) AS center_cell
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
  WHERE lip.build_id = (SELECT bridge_build_id FROM params)
    AND lip.county_fips = (SELECT county_fips FROM params)
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
),
subject_stats AS (
  SELECT
    COUNT(DISTINCT property_key) AS subject_properties,
    COUNT(DISTINCT loan_key) AS subject_loans,
    COUNT(DISTINCT IFF(loan_property_count > 1, loan_key, NULL))
      AS multi_property_loans,
    COUNT(DISTINCT center_cell) AS subject_center_cells
  FROM subject_rows
),
work_cells AS (
  SELECT DISTINCT
    H3_STRING_TO_INT(cell.value::TEXT) AS h3_r8_int
  FROM (
    SELECT DISTINCT center_cell
    FROM subject_rows
  ) centers,
  LATERAL FLATTEN(
    input => H3_GRID_DISK(
      centers.center_cell,
      (SELECT halo_k FROM params)
    )
  ) cell
),
work_stats AS (
  SELECT COUNT(*) AS work_cells
  FROM work_cells
),
source_summary AS (
  SELECT
    required.evidence_class,
    required.source_system,
    required.source_table,
    required.release,
    TO_DATE(required.release_dt) AS release_dt,
    required.dataset,
    COUNT(keys.provider_feature_id) AS feature_rows,
    COUNT(DISTINCT keys.provider_feature_id) AS distinct_features,
    COUNT(DISTINCT keys.h3_r8_int) AS occupied_work_cells
  FROM required_sources required
  LEFT JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_GEOMETRY_HOT_KEYS keys
    ON keys.source_system = required.source_system
   AND keys.source_table = required.source_table
   AND COALESCE(keys.release, '<null>')
     = COALESCE(required.release, '<null>')
   AND keys.release_dt = TO_DATE(required.release_dt)
   AND keys.dataset = required.dataset
   AND keys.key_validation_status = 'valid'
   AND keys.h3_r8_int IN (SELECT h3_r8_int FROM work_cells)
  GROUP BY
    required.evidence_class,
    required.source_system,
    required.source_table,
    required.release,
    required.release_dt,
    required.dataset
),
guard_failures AS (
  SELECT 'no_subject_properties' AS failure_reason
  FROM subject_stats
  WHERE subject_properties = 0
  UNION ALL
  SELECT 'no_subject_center_cells'
  FROM subject_stats
  WHERE subject_center_cells = 0
  UNION ALL
  SELECT 'no_work_cells'
  FROM work_stats
  WHERE work_cells = 0
  UNION ALL
  SELECT 'halo_cardinality_impossible'
  FROM subject_stats, work_stats
  WHERE work_cells > subject_center_cells * 7
  UNION ALL
  SELECT 'required_source_empty:' || evidence_class
  FROM source_summary
  WHERE distinct_features = 0 OR occupied_work_cells = 0
  UNION ALL
  SELECT 'source_row_duplicate_inflation:' || evidence_class
  FROM source_summary
  WHERE feature_rows <> distinct_features
),
guard AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    LISTAGG(failure_reason, '|') WITHIN GROUP (ORDER BY failure_reason)
      AS refusal_reason
  FROM guard_failures
)
SELECT
  'canon_geo_e5_thin_tier_readiness.v0' AS row_contract,
  params.geography,
  params.county_fips,
  params.bridge_build_id,
  params.h3_resolution,
  params.halo_k,
  params.measurement_scope,
  subject_stats.subject_properties,
  subject_stats.subject_loans,
  subject_stats.multi_property_loans,
  subject_stats.subject_center_cells,
  work_stats.work_cells,
  source_summary.evidence_class,
  source_summary.source_system,
  source_summary.source_table,
  source_summary.release,
  source_summary.release_dt,
  source_summary.dataset,
  source_summary.feature_rows,
  source_summary.distinct_features,
  source_summary.occupied_work_cells,
  guard.guard_status,
  guard.refusal_reason
FROM params
CROSS JOIN subject_stats
CROSS JOIN work_stats
CROSS JOIN guard
CROSS JOIN source_summary
ORDER BY source_summary.evidence_class;
