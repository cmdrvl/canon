-- bd-1q5y: remaining-five condo geometry bridge probe for the frozen retained
-- E4 population after PLUTO lot-vintage widening.
--
-- This query intentionally separates exact unit-BBL geometry from usable
-- billing-BBL candidate geometry:
-- * residual_unit_bbls is the unchanged 331-BBL residual after
--   e4_pluto_vintage_reach.sql adds historical PLUTO lots as candidates.
-- * PAD 26B is bound by release and maps unit lots to billing BBLs through
--   exact rows or LOW/HIGH condo ranges.
-- * MapPLUTO geometry is read by canonical BBL using SPLIT_PART(BBL, '.', 1)
--   because the geometry planes render BBLs as values like 1010297502.0.
--
-- Candidate-universe boundary: billing-lot geometry can supply a candidate
-- geometry for a typed condo representation bridge. It is not unit-lot geometry
-- and is not evidence that the unit lots are collateral.
WITH
residual_unit_bbls(case_id, truth_plane, unit_bbl) AS (
  SELECT 'h7-subject:non-round:655a127dc453d12220696e0a0f76929d2ab8dd91ebd1dbe363173e50408fb34b',
    'non_round_amount_date_legal_borough',
    TO_VARCHAR(4067971301 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 145))
  UNION ALL
  SELECT 'h7-subject:non-round:68a1a4ced3f7c16edc483b877013238d0e2c26692c9ef0a5a721e6bb25872612',
    'non_round_amount_date_legal_borough',
    '1012741304'
  UNION ALL
  SELECT 'h7-subject:non-round:68a1a4ced3f7c16edc483b877013238d0e2c26692c9ef0a5a721e6bb25872612',
    'non_round_amount_date_legal_borough',
    '1013261075'
  UNION ALL
  SELECT 'h7-subject:non-round:e3a5228d84bb6bf01ff03e2849e4a996f223cb1fcff79eff04a65d84dcfa8deb',
    'non_round_amount_date_legal_borough',
    TO_VARCHAR(4050141101 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 10))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:0a6ff10eaf74e3ff8cde56399cf9297813f3833885064be70351eae286e6da9b',
    'round_exact_lender_party',
    TO_VARCHAR(1000281301 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 2))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291102 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 10))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291113 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 5))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    '1010291119'
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291121 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 29))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291151 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 5))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291157 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 5))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291163 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 13))
  UNION ALL
  SELECT 'h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6',
    'round_exact_lender_party',
    TO_VARCHAR(1010291177 + SEQ4())
  FROM TABLE(GENERATOR(ROWCOUNT => 104))
),
pad_matches AS (
  SELECT
    r.case_id,
    r.truth_plane,
    r.unit_bbl,
    p.release AS pad_release,
    p.release_dt AS pad_release_dt,
    p.source_row_number AS pad_source_row_number,
    p.source_zip_sha256 AS pad_source_zip_sha256,
    p.parser_version AS pad_parser_version,
    p.license_terms AS pad_license_terms,
    p.attribution_text AS pad_attribution_text,
    p.source_filename AS pad_source_filename,
    p.bbl_key AS pad_bbl_key,
    p.low_bbl_key,
    p.high_bbl_key,
    p.billing_bbl_key,
    p.condo_flag,
    p.condo_number,
    CASE
      WHEN p.bbl_key = r.unit_bbl THEN 'exact_bbl_key'
      WHEN p.low_bbl_key <= r.unit_bbl AND p.high_bbl_key >= r.unit_bbl THEN 'range_contains'
      ELSE 'unmatched'
    END AS pad_match_kind
  FROM residual_unit_bbls r
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT p
    ON p.release = '26B'
   AND SUBSTR(r.unit_bbl, 1, 1) = TO_VARCHAR(p.boro)
   AND (
      p.bbl_key = r.unit_bbl
      OR (p.low_bbl_key <= r.unit_bbl AND p.high_bbl_key >= r.unit_bbl)
    )
),
pad_one AS (
  SELECT *
  FROM pad_matches
  QUALIFY ROW_NUMBER() OVER (
    PARTITION BY unit_bbl
    ORDER BY CASE pad_match_kind
        WHEN 'exact_bbl_key' THEN 0
        WHEN 'range_contains' THEN 1
        ELSE 2
      END,
      pad_source_row_number,
      billing_bbl_key
  ) = 1
),
geom_unit AS (
  SELECT
    SPLIT_PART(bbl, '.', 1) AS canonical_bbl,
    release,
    release_dt,
    source_row_number,
    objectid,
    geom_wgs84_sha256,
    source_geom_wkb_sha256,
    transform_execution_id
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT
  WHERE SPLIT_PART(bbl, '.', 1) IN (SELECT unit_bbl FROM residual_unit_bbls)
),
geom_billing AS (
  SELECT
    SPLIT_PART(bbl, '.', 1) AS canonical_bbl,
    release,
    release_dt,
    variant,
    source_row_number,
    objectid,
    source_filename,
    geom_wgs84_sha256,
    source_geom_wkb_sha256,
    geometry_evidence_contract_version,
    transform_execution_id,
    transform_definition_id,
    source_vertex_count,
    geom_crs,
    source_geom_crs,
    address,
    bldgclass,
    landuse,
    lotarea,
    bldgarea,
    is_current_release
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT
  WHERE SPLIT_PART(bbl, '.', 1) IN (
    SELECT DISTINCT billing_bbl_key
    FROM pad_one
    WHERE billing_bbl_key IS NOT NULL
  )
),
billing_evidence AS (
  SELECT
    SPLIT_PART(bbl, '.', 1) AS canonical_bbl,
    release,
    release_dt,
    source_row_number,
    objectid,
    source_archive_sha256,
    source_archive_s3_key,
    source_geometry_validity,
    source_crs_identifier,
    source_crs_wkt2_sha256
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOMETRY_EVIDENCE_EXT
  WHERE SPLIT_PART(bbl, '.', 1) IN (
    SELECT DISTINCT billing_bbl_key
    FROM pad_one
    WHERE billing_bbl_key IS NOT NULL
  )
),
billing_geometry AS (
  SELECT
    g.*,
    e.source_archive_sha256,
    e.source_archive_s3_key,
    e.source_geometry_validity,
    e.source_crs_identifier,
    e.source_crs_wkt2_sha256
  FROM geom_billing g
  LEFT JOIN billing_evidence e
    ON e.canonical_bbl = g.canonical_bbl
   AND e.release = g.release
   AND e.release_dt = g.release_dt
   AND e.source_row_number = g.source_row_number
   AND COALESCE(TO_VARCHAR(e.objectid), '') = COALESCE(TO_VARCHAR(g.objectid), '')
),
case_bridge AS (
  SELECT
    p.case_id,
    p.truth_plane,
    COUNT(DISTINCT p.unit_bbl) AS residual_unit_bbls,
    COUNT(DISTINCT p.billing_bbl_key) AS billing_truth_members_after_pad_bridge,
    COUNT(DISTINCT bg.canonical_bbl) AS billing_truth_members_with_mappluto_geometry,
    LISTAGG(DISTINCT p.billing_bbl_key, '|')
      WITHIN GROUP (ORDER BY p.billing_bbl_key) AS billing_bbls,
    LISTAGG(DISTINCT bg.canonical_bbl, '|')
      WITHIN GROUP (ORDER BY bg.canonical_bbl) AS billing_bbls_with_geometry,
    COUNT(DISTINCT CASE WHEN gu.canonical_bbl IS NOT NULL THEN p.unit_bbl END)
      AS unit_bbls_with_geometry
  FROM pad_one p
  LEFT JOIN billing_geometry bg ON bg.canonical_bbl = p.billing_bbl_key
  LEFT JOIN geom_unit gu ON gu.canonical_bbl = p.unit_bbl
  GROUP BY p.case_id, p.truth_plane
)
SELECT
  'summary' AS row_kind,
  OBJECT_CONSTRUCT(
    'residual_cases', (SELECT COUNT(DISTINCT case_id) FROM residual_unit_bbls),
    'residual_unit_bbls', (SELECT COUNT(*) FROM residual_unit_bbls),
    'distinct_residual_unit_bbls', (SELECT COUNT(DISTINCT unit_bbl) FROM residual_unit_bbls),
    'units_with_pad_row', (SELECT COUNT(DISTINCT unit_bbl) FROM pad_one),
    'units_with_billing_bbl',
      (SELECT COUNT(DISTINCT CASE WHEN billing_bbl_key IS NOT NULL THEN unit_bbl END) FROM pad_one),
    'distinct_billing_bbls', (SELECT COUNT(DISTINCT billing_bbl_key) FROM pad_one),
    'units_matched_by_exact_key',
      (SELECT COUNT(DISTINCT CASE WHEN pad_match_kind = 'exact_bbl_key' THEN unit_bbl END) FROM pad_one),
    'units_matched_by_range',
      (SELECT COUNT(DISTINCT CASE WHEN pad_match_kind = 'range_contains' THEN unit_bbl END) FROM pad_one),
    'unit_bbls_with_mappluto_geom_v3', (SELECT COUNT(DISTINCT canonical_bbl) FROM geom_unit),
    'unit_bbl_geom_v3_rows', (SELECT COUNT(*) FROM geom_unit),
    'billing_bbls_with_mappluto_geom_v3', (SELECT COUNT(DISTINCT canonical_bbl) FROM billing_geometry),
    'mappluto_geom_v3_billing_rows', (SELECT COUNT(*) FROM billing_geometry),
    'mappluto_geom_v3_billing_releases',
      (SELECT LISTAGG(DISTINCT release, '|') WITHIN GROUP (ORDER BY release) FROM billing_geometry),
    'mappluto_geom_v3_rows_with_source_archive_sha256',
      (SELECT COUNT_IF(source_archive_sha256 IS NOT NULL) FROM billing_geometry),
    'mappluto_geom_v3_rows_with_geom_wgs84_sha256',
      (SELECT COUNT_IF(geom_wgs84_sha256 IS NOT NULL) FROM billing_geometry),
    'mappluto_geom_v3_rows_with_source_geom_wkb_sha256',
      (SELECT COUNT_IF(source_geom_wkb_sha256 IS NOT NULL) FROM billing_geometry),
    'mappluto_geom_v3_rows_with_transform_execution_id',
      (SELECT COUNT_IF(transform_execution_id IS NOT NULL) FROM billing_geometry)
  ) AS payload
UNION ALL
SELECT
  'case_bridge' AS row_kind,
  OBJECT_CONSTRUCT(
    'case_id', case_id,
    'truth_plane', truth_plane,
    'residual_unit_bbls', residual_unit_bbls,
    'billing_truth_members_after_pad_bridge', billing_truth_members_after_pad_bridge,
    'billing_truth_members_with_mappluto_geometry', billing_truth_members_with_mappluto_geometry,
    'billing_bbls', billing_bbls,
    'billing_bbls_with_geometry', billing_bbls_with_geometry,
    'unit_bbls_with_geometry', unit_bbls_with_geometry
  ) AS payload
FROM case_bridge
UNION ALL
SELECT
  'billing_geometry' AS row_kind,
  OBJECT_CONSTRUCT(
    'bbl', canonical_bbl,
    'release', release,
    'release_dt', release_dt,
    'variant', variant,
    'source_row_number', source_row_number,
    'source_filename', source_filename,
    'source_archive_sha256', source_archive_sha256,
    'source_archive_s3_key', source_archive_s3_key,
    'geom_wgs84_sha256', geom_wgs84_sha256,
    'source_geom_wkb_sha256', source_geom_wkb_sha256,
    'geometry_evidence_contract_version', geometry_evidence_contract_version,
    'transform_execution_id', transform_execution_id,
    'transform_definition_id', transform_definition_id,
    'source_geometry_validity', source_geometry_validity,
    'source_vertex_count', source_vertex_count,
    'geom_crs', geom_crs,
    'source_geom_crs', source_geom_crs,
    'source_crs_identifier', source_crs_identifier,
    'source_crs_wkt2_sha256', source_crs_wkt2_sha256,
    'address', address,
    'bldgclass', bldgclass,
    'landuse', landuse,
    'lotarea', lotarea,
    'bldgarea', bldgarea,
    'is_current_release', is_current_release
  ) AS payload
FROM billing_geometry
ORDER BY row_kind, payload:bbl::TEXT, payload:release::TEXT, payload:case_id::TEXT;
